//! MCP Client for local stdio servers
//!
//! Handles JSON-RPC communication with a single MCP server.
//! Uses a background receive loop to avoid race conditions.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, info};

use super::config::McpServerConfig;
use super::protocol::{
    ClientCapabilities, ClientInfo, InitializeParams, InitializeResult, McpRequest, McpResponse,
    McpToolDef, McpToolResult, ToolCallParams, ToolCallResult, ToolsListResult,
};
use super::transport::StdioTransport;

const PROTOCOL_VERSION: &str = "2024-11-05";
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// MCP client for a local server
pub struct McpClient {
    name: String,
    transport: Arc<StdioTransport>,
    next_id: AtomicI64,
    /// Pending request handlers
    pending: Arc<RwLock<HashMap<i64, oneshot::Sender<Result<Value>>>>>,
    /// Cached tools
    tools: RwLock<Vec<McpToolDef>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl McpClient {
    /// Connect to a local MCP server
    pub async fn connect(
        name: &str,
        config: &McpServerConfig,
        working_dir: &Path,
    ) -> Result<Self> {
        let McpServerConfig::Local { command, args, env } = config else {
            return Err(anyhow!("McpClient only handles local servers"));
        };

        info!("Connecting to MCP server: {}", name);

        let transport = Arc::new(
            StdioTransport::spawn(command, args, env, working_dir).await?,
        );

        let pending: Arc<RwLock<HashMap<i64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Start background receive loop
        let recv_transport = Arc::clone(&transport);
        let recv_pending = Arc::clone(&pending);
        let recv_name = name.to_string();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("MCP client {} shutting down receive loop", recv_name);
                        break;
                    }
                    result = recv_transport.receive() => {
                        match result {
                            Ok(message) => {
                                if let Err(e) = handle_message(&message, &recv_pending).await {
                                    error!("MCP {} message error: {}", recv_name, e);
                                }
                            }
                            Err(e) => {
                                error!("MCP {} receive error: {}", recv_name, e);
                                // Fail all pending requests
                                let mut pending = recv_pending.write().await;
                                for (_, tx) in pending.drain() {
                                    let _ = tx.send(Err(anyhow!("Connection lost")));
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });

        let client = Self {
            name: name.to_string(),
            transport,
            next_id: AtomicI64::new(1),
            pending,
            tools: RwLock::new(Vec::new()),
            shutdown_tx: Some(shutdown_tx),
        };

        Ok(client)
    }

    /// Initialize the MCP connection (required before using tools)
    pub async fn initialize(&self) -> Result<InitializeResult> {
        info!("Initializing MCP connection for {}", self.name);

        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: ClientInfo {
                name: "krusty".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        debug!("Sending initialize request to {}", self.name);
        let result: InitializeResult = self
            .request("initialize", Some(serde_json::to_value(params)?))
            .await
            .map_err(|e| {
                error!("MCP {} initialize failed: {}", self.name, e);
                e
            })?;

        info!(
            "MCP {} initialized (protocol: {})",
            self.name, result.protocol_version
        );

        // Send initialized notification
        self.notify("notifications/initialized", None).await?;

        Ok(result)
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Vec<McpToolDef>> {
        let result: ToolsListResult = self.request("tools/list", None).await?;
        info!("MCP {} has {} tools", self.name, result.tools.len());

        // Log tool schemas for debugging provider compatibility
        for tool in &result.tools {
            debug!(
                "MCP {} tool '{}' schema: {}",
                self.name,
                tool.name,
                serde_json::to_string(&tool.input_schema).unwrap_or_default()
            );
        }

        // Cache tools
        *self.tools.write().await = result.tools.clone();

        Ok(result.tools)
    }

    /// Call a tool
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<McpToolResult> {
        let params = ToolCallParams {
            name: name.to_string(),
            arguments: if arguments.is_null() {
                None
            } else {
                Some(arguments)
            },
        };

        let result: ToolCallResult = self
            .request("tools/call", Some(serde_json::to_value(params)?))
            .await?;

        Ok(result.into())
    }

    /// Get cached tools
    pub async fn get_tools(&self) -> Vec<McpToolDef> {
        self.tools.read().await.clone()
    }

    /// Get server name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if connection is alive
    pub async fn is_alive(&self) -> bool {
        self.transport.is_alive().await
    }

    /// Send a request and wait for response
    async fn request<R: for<'de> serde::Deserialize<'de>>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = McpRequest::new(id, method, params);
        let json = serde_json::to_string(&request)?;

        debug!("MCP {} request [{}]: {}", self.name, id, method);

        // Create response channel
        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(id, tx);

        // Send request
        self.transport.send(&json).await?;

        // Wait for response with timeout
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS),
            rx,
        )
        .await;

        match result {
            Ok(Ok(Ok(value))) => Ok(serde_json::from_value(value)?),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(anyhow!("Request cancelled")),
            Err(_) => {
                // Remove pending request on timeout
                self.pending.write().await.remove(&id);
                Err(anyhow!("Request timed out after {}s", REQUEST_TIMEOUT_SECS))
            }
        }
    }

    /// Send a notification (no response expected)
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        #[derive(serde::Serialize)]
        struct Notification {
            jsonrpc: &'static str,
            method: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            params: Option<Value>,
        }

        let notification = Notification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&notification)?;
        debug!("MCP {} notify: {}", self.name, method);
        self.transport.send(&json).await
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Signal shutdown to receive loop
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
    }
}

/// Handle an incoming message (called by receive loop)
async fn handle_message(
    message: &str,
    pending: &RwLock<HashMap<i64, oneshot::Sender<Result<Value>>>>,
) -> Result<()> {
    let response: McpResponse = serde_json::from_str(message)?;

    // Check if it's a response to a request
    if let Some(id) = response.id {
        let mut pending = pending.write().await;
        if let Some(tx) = pending.remove(&id) {
            if let Some(error) = response.error {
                let _ = tx.send(Err(anyhow!("MCP error {}: {}", error.code, error.message)));
            } else {
                let _ = tx.send(Ok(response.result.unwrap_or(Value::Null)));
            }
        }
        return Ok(());
    }

    // Handle notifications (server → client)
    if let Some(method) = &response.method {
        debug!("MCP notification: {}", method);
        // We don't need to handle notifications currently
    }

    Ok(())
}
