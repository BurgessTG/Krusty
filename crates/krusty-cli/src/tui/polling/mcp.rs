//! MCP status channel polling
//!
//! Handles status updates from background MCP connection tasks.

use std::sync::Arc;

use crate::ai::types::AiTool;
use crate::tools::ToolRegistry;
use crate::tui::popups::mcp_browser::McpBrowserPopup;
use crate::tui::utils::AsyncChannels;

use super::PollResult;

/// Poll MCP status updates from background connection tasks
#[allow(dead_code)]
pub fn poll_mcp_status(
    channels: &mut AsyncChannels,
    mcp_popup: &mut McpBrowserPopup,
    cached_ai_tools: &mut Vec<AiTool>,
    tool_registry: &Arc<ToolRegistry>,
    mut refresh_popup: impl FnMut(),
) -> PollResult {
    let mut result = PollResult::new();

    let Some(mut rx) = channels.mcp_status.take() else {
        return result;
    };

    loop {
        match rx.try_recv() {
            Ok(update) => {
                result.needs_redraw = true;

                // Update popup status message
                let status_msg = if update.success {
                    format!("✓ {}", update.message)
                } else {
                    format!("✗ {}", update.message)
                };
                mcp_popup.set_status(status_msg);

                // Refresh server list to show updated state
                refresh_popup();

                // Refresh cached AI tools so new MCP tools are sent to the API
                if update.success {
                    *cached_ai_tools = futures::executor::block_on(tool_registry.get_ai_tools());
                    tracing::info!(
                        "Refreshed AI tools after MCP update, total: {}",
                        cached_ai_tools.len()
                    );
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                channels.mcp_status = Some(rx);
                break;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                break;
            }
        }
    }

    result
}
