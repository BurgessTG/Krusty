//! MCP Manager - manages local MCP server connections
//!
//! Simple manager for local stdio servers. Remote servers are handled
//! by passing them to the Anthropic API's MCP Connector.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::client::McpClient;
use super::config::{McpConfig, McpServerConfig, RemoteMcpServer};
use super::protocol::{McpToolDef, McpToolResult};

/// Server status
#[derive(Debug, Clone, PartialEq)]
pub enum McpServerStatus {
    Disconnected,
    Connected,
    Error(String),
}

impl std::fmt::Display for McpServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpServerStatus::Disconnected => write!(f, "disconnected"),
            McpServerStatus::Connected => write!(f, "connected"),
            McpServerStatus::Error(e) => write!(f, "error: {}", e),
        }
    }
}

/// Server info for UI
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub server_type: String, // "stdio" or "remote"
    pub status: McpServerStatus,
    pub tool_count: usize,
    pub tools: Vec<McpToolDef>,
    pub error: Option<String>,
}

/// MCP Manager
pub struct McpManager {
    /// Connected local clients
    clients: RwLock<HashMap<String, Arc<McpClient>>>,
    /// Server configurations
    configs: RwLock<HashMap<String, McpServerConfig>>,
    /// Remote servers (for API)
    remote_servers: RwLock<Vec<RemoteMcpServer>>,
    /// Working directory
    working_dir: PathBuf,
}

impl McpManager {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            configs: RwLock::new(HashMap::new()),
            remote_servers: RwLock::new(Vec::new()),
            working_dir,
        }
    }

    /// Load configuration from .mcp.json
    pub async fn load_config(&self) -> Result<()> {
        let config = McpConfig::load(&self.working_dir).await?;

        let mut configs = self.configs.write().await;
        *configs = config.servers().await;

        // Store remote servers for API
        *self.remote_servers.write().await = config.remote_servers_for_api().await;

        let local_count = configs.values().filter(|c| c.is_local()).count();
        let remote_count = configs.values().filter(|c| c.is_remote()).count();

        info!(
            "Loaded MCP config: {} local, {} remote servers",
            local_count, remote_count
        );

        Ok(())
    }

    /// Connect to all local servers
    pub async fn connect_all(&self) -> Result<()> {
        let configs: Vec<_> = {
            let configs = self.configs.read().await;
            configs
                .iter()
                .filter(|(_, c)| c.is_local())
                .map(|(n, c)| (n.clone(), c.clone()))
                .collect()
        };

        info!("Connecting to {} local MCP servers", configs.len());
        for (name, _) in configs {
            info!("Attempting to connect to MCP server: {}", name);
            if let Err(e) = self.connect(&name).await {
                warn!("Failed to connect to MCP server {}: {:?}", name, e);
            }
        }

        Ok(())
    }

    /// Connect to a specific local server
    pub async fn connect(&self, name: &str) -> Result<()> {
        let config = {
            let configs = self.configs.read().await;
            configs.get(name).cloned()
        };

        let Some(config) = config else {
            return Err(anyhow::anyhow!("Unknown server: {}", name));
        };

        if config.is_remote() {
            return Err(anyhow::anyhow!(
                "Server {} is remote - handled by Anthropic API",
                name
            ));
        }

        // Disconnect first if already connected
        self.disconnect(name).await;

        // Connect
        let client = McpClient::connect(name, &config, &self.working_dir).await?;

        // Initialize
        client.initialize().await?;

        // Get tools
        client.list_tools().await?;

        let client = Arc::new(client);
        self.clients.write().await.insert(name.to_string(), client);

        info!("Connected to MCP server: {}", name);
        Ok(())
    }

    /// Disconnect from a server
    pub async fn disconnect(&self, name: &str) {
        if self.clients.write().await.remove(name).is_some() {
            info!("Disconnected from MCP server: {}", name);
        }
    }

    /// Get all tools from connected local servers
    pub async fn get_all_tools(&self) -> Vec<(String, McpToolDef)> {
        let clients = self.clients.read().await;
        let mut tools = Vec::new();

        for (name, client) in clients.iter() {
            for tool in client.get_tools().await {
                tools.push((name.clone(), tool));
            }
        }

        tools
    }

    /// Call a tool on a local server
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<McpToolResult> {
        let clients = self.clients.read().await;
        let client = clients
            .get(server)
            .ok_or_else(|| anyhow::anyhow!("Server not connected: {}", server))?;

        client.call_tool(tool, arguments).await
    }

    /// Get server info for UI
    pub async fn list_servers(&self) -> Vec<McpServerInfo> {
        let configs = self.configs.read().await;
        let clients = self.clients.read().await;

        let mut servers = Vec::new();

        for (name, config) in configs.iter() {
            let (status, tool_count, tools, error) = if config.is_local() {
                if let Some(client) = clients.get(name) {
                    let t = client.get_tools().await;
                    if client.is_alive().await {
                        (McpServerStatus::Connected, t.len(), t, None)
                    } else {
                        (
                            McpServerStatus::Error("Process died".to_string()),
                            0,
                            Vec::new(),
                            Some("Process died".to_string()),
                        )
                    }
                } else {
                    (McpServerStatus::Disconnected, 0, Vec::new(), None)
                }
            } else {
                // Remote servers are always "connected" (handled by API)
                (McpServerStatus::Connected, 0, Vec::new(), None)
            };

            servers.push(McpServerInfo {
                name: name.clone(),
                server_type: config.transport_type().to_string(),
                status,
                tool_count,
                tools,
                error,
            });
        }

        servers.sort_by(|a, b| a.name.cmp(&b.name));
        servers
    }

    /// Get remote servers for Anthropic API
    pub async fn get_remote_servers(&self) -> Vec<RemoteMcpServer> {
        self.remote_servers.read().await.clone()
    }

    /// Check if any servers are configured
    pub async fn has_servers(&self) -> bool {
        !self.configs.read().await.is_empty()
    }

    /// Get a connected client
    pub async fn get_client(&self, name: &str) -> Option<Arc<McpClient>> {
        self.clients.read().await.get(name).cloned()
    }
}
