//! Channel Polling
//!
//! Extracted polling logic from App. These functions poll async channels
//! for background task results and update the appropriate state.
//!
//! This module reduces the App god object by extracting ~500 lines of
//! channel polling logic into focused, testable functions.

mod bash;
mod blocks;
mod mcp;
mod oauth;
mod processes;

// Currently integrated
pub use bash::poll_bash_output;
pub use blocks::{poll_build_progress, poll_explore_progress};

// Future integration (require more refactoring due to borrow conflicts)
#[allow(unused_imports)]
pub use blocks::poll_init_exploration;
#[allow(unused_imports)]
pub use mcp::poll_mcp_status;
#[allow(unused_imports)]
pub use oauth::poll_oauth_status;
#[allow(unused_imports)]
pub use processes::poll_background_processes;

#[allow(unused_imports)]
use crate::tui::utils::AsyncChannels;

/// Result of a polling operation that may trigger UI updates
#[derive(Debug, Default)]
pub struct PollResult {
    /// Whether any data was received that requires a redraw
    pub needs_redraw: bool,
    /// Messages to append to the conversation (for future use when all pollers return PollResult)
    #[allow(dead_code)]
    pub messages: Vec<(String, String)>,
}

impl PollResult {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn with_redraw(mut self) -> Self {
        self.needs_redraw = true;
        self
    }

    #[allow(dead_code)]
    pub fn with_message(mut self, role: impl Into<String>, content: impl Into<String>) -> Self {
        self.messages.push((role.into(), content.into()));
        self
    }

    /// Merge another result into this one
    #[allow(dead_code)]
    pub fn merge(&mut self, other: PollResult) {
        self.needs_redraw |= other.needs_redraw;
        self.messages.extend(other.messages);
    }
}

/// Context for polling operations that need access to services
#[allow(dead_code)]
pub struct PollContext<'a> {
    pub channels: &'a mut AsyncChannels,
}
