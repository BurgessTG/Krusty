//! MCP Tool wrapper
//!
//! Wraps MCP tools as our Tool trait for seamless integration.
//!
//! NOTE: MCP tools execute on external servers and bypass Krusty's sandbox.
//! When sandbox_root is configured, a warning is logged for visibility.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::warn;

use super::manager::McpManager;
use super::protocol::{format_mcp_result, McpToolDef};
use crate::tools::registry::{Tool, ToolContext, ToolResult};

/// Wraps an MCP tool as our Tool trait
pub struct McpTool {
    server_name: String,
    tool_name: String,
    full_name: String,
    definition: McpToolDef,
    manager: Arc<McpManager>,
}

impl McpTool {
    pub fn new(server_name: String, definition: McpToolDef, manager: Arc<McpManager>) -> Self {
        let tool_name = definition.name.clone();
        let full_name = format!("mcp__{}_{}", server_name, tool_name);

        Self {
            server_name,
            tool_name,
            full_name,
            definition,
            manager,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.full_name
    }

    fn description(&self) -> &str {
        self.definition.description.as_deref().unwrap_or("MCP tool")
    }

    fn parameters_schema(&self) -> Value {
        self.definition.input_schema.clone()
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> ToolResult {
        // Warn when MCP tools are used in sandboxed mode - they bypass sandbox restrictions
        if ctx.sandbox_root.is_some() {
            warn!(
                "MCP tool '{}' executing in sandboxed context - MCP servers bypass sandbox restrictions",
                self.full_name
            );
        }

        match self
            .manager
            .call_tool(&self.server_name, &self.tool_name, params)
            .await
        {
            Ok(result) => ToolResult {
                output: format_mcp_result(&result),
                is_error: result.is_error,
            },
            Err(e) => ToolResult {
                output: format!("MCP error: {}", e),
                is_error: true,
            },
        }
    }
}

/// Register all MCP tools from connected servers
pub async fn register_mcp_tools(manager: Arc<McpManager>, registry: &crate::tools::ToolRegistry) {
    let tools = manager.get_all_tools().await;

    for (server_name, tool_def) in tools {
        let mcp_tool = Arc::new(McpTool::new(server_name, tool_def, manager.clone()));
        registry.register(mcp_tool).await;
    }
}
