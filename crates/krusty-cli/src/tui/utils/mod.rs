//! Utilities for the TUI
//!
//! Common helper functions and types used throughout the TUI.

mod channels;
mod oauth;
mod syntax;
mod text;
mod title;
mod worktree;

pub use channels::{AsyncChannels, InitExplorationResult, McpStatusUpdate, SummarizationUpdate, TitleUpdate};
pub use syntax::highlight_code;
pub use text::{count_wrapped_lines, truncate_ellipsis, wrap_line, wrap_text};
pub use title::{TitleAction, TitleEditor};
pub use worktree::AppWorktreeDelegate;
