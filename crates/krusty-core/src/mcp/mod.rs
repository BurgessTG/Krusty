//! MCP (Model Context Protocol) client implementation
//!
//! Supports two types of MCP servers:
//! - Local (stdio): We spawn the process and act as MCP client
//! - Remote (url): Passed to Anthropic API's MCP Connector
//!
//! Local servers are managed here. Remote servers are passed to the API.

mod client;
mod config;
mod manager;
mod protocol;
pub mod tool;
mod transport;

pub use config::{McpConfig, McpServerConfig, RemoteMcpServer};
pub use manager::{McpManager, McpServerInfo, McpServerStatus};
pub use protocol::{McpContent, McpToolDef, McpToolResult};
pub use tool::McpTool;
