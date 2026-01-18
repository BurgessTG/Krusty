//! MCP protocol types (JSON-RPC 2.0)
//!
//! Defines the wire format for MCP communication.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request
#[derive(Debug, Serialize)]
pub struct McpRequest {
    pub jsonrpc: &'static str,
    pub id: i64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpRequest {
    pub fn new(id: i64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
pub struct McpResponse {
    #[serde(rename = "jsonrpc")]
    pub _jsonrpc: String,
    pub id: Option<i64>,
    pub result: Option<Value>,
    pub error: Option<McpError>,
    /// For notifications
    #[serde(default)]
    pub method: Option<String>,
    /// Notification params (protocol field, not directly used)
    #[serde(default)]
    pub _params: Option<Value>,
}

/// JSON-RPC error
#[derive(Debug, Deserialize)]
pub struct McpError {
    pub code: i64,
    pub message: String,
    /// Additional error data (protocol field, not directly used)
    #[serde(default, rename = "data")]
    pub _data: Option<Value>,
}

/// MCP tool definition from tools/list
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// MCP tool call result
#[derive(Debug, Clone)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    pub is_error: bool,
}

/// Content types returned by MCP tools
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    Resource {
        uri: String,
        #[serde(default)]
        text: Option<String>,
    },
}

impl std::fmt::Display for McpContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpContent::Text { text } => write!(f, "{}", text),
            McpContent::Image { mime_type, .. } => write!(f, "[Image: {}]", mime_type),
            McpContent::Resource { uri, text } => {
                if let Some(t) = text {
                    write!(f, "{}\n{}", uri, t)
                } else {
                    write!(f, "{}", uri)
                }
            }
        }
    }
}

/// Initialize request params
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

/// Client capabilities
#[derive(Debug, Default, Serialize)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    pub list_changed: bool,
}

/// Client info
#[derive(Debug, Serialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Initialize response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(default)]
    pub server_info: Option<ServerInfo>,
}

/// Server capabilities
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    pub resources: Option<Value>,
    #[serde(default)]
    pub prompts: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

/// Server info
#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Tools list response
#[derive(Debug, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<McpToolDef>,
}

/// Tool call params
#[derive(Debug, Serialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// Tool call result (from server)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}

impl From<ToolCallResult> for McpToolResult {
    fn from(result: ToolCallResult) -> Self {
        Self {
            content: result.content,
            is_error: result.is_error,
        }
    }
}

/// Format MCP tool result for display
pub fn format_mcp_result(result: &McpToolResult) -> String {
    result
        .content
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
