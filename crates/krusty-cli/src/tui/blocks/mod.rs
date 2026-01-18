//! Stream blocks - modular widgets for chat stream rendering
//!
//! Each block type implements the StreamBlock trait and handles its own
//! rendering, interaction, and state management.
//!
//! Blocks support partial visibility via ClipContext - when scrolled partially
//! off-screen, they receive clip info to render borders correctly.

pub mod bash;
pub mod build;
pub mod edit;
pub mod explore;
pub mod read;
pub mod terminal_pane;
pub mod thinking;
pub mod tool_result;
pub mod web_search;
pub mod write;

use crossterm::event::Event;
use ratatui::{buffer::Buffer, layout::Rect};

use crate::tui::themes::Theme;

/// Clipping context for partially visible blocks
///
/// When a block is scrolled partially off-screen, this tells it which
/// portions are clipped so it can skip drawing borders appropriately.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClipContext {
    /// Lines clipped from block's top (0 = fully visible from top)
    pub clip_top: u16,
    /// Lines clipped from block's bottom (0 = fully visible at bottom)
    pub clip_bottom: u16,
}

/// Types of blocks that can be hit-tested
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    Thinking,
    Bash,
    ToolResult,
    Read,
    Edit,
    Write,
    TerminalPane,
    WebSearch,
    Explore,
    Build,
}

/// Result of a block hit test
#[derive(Debug, Clone)]
pub struct BlockHitResult {
    /// Type of block that was hit
    pub block_type: BlockType,
    /// Index into the block collection
    pub index: usize,
    /// Screen area of the block
    pub area: Rect,
    /// Clipping context if block is partially visible
    pub clip: Option<ClipContext>,
}

/// Result of handling an event
#[derive(Debug, Clone)]
pub enum EventResult {
    /// Block consumed the event
    Consumed,
    /// Block ignored the event, pass to parent
    Ignored,
    /// Block triggered an action
    Action(BlockEvent),
}

/// Events that blocks can emit
#[derive(Debug, Clone)]
pub enum BlockEvent {
    /// Request focus on this block
    RequestFocus,
    /// Block was expanded
    Expanded,
    /// Block was collapsed
    Collapsed,
    /// Block requests to be closed/removed
    Close,
    /// Block pinned state changed
    Pinned(bool),
    /// Toggle global diff display mode (unified <-> side-by-side)
    ToggleDiffMode,
}

/// Core trait for all stream blocks
pub trait StreamBlock: Send + Sync {
    /// Calculate height needed given a width
    fn height(&self, width: u16, theme: &Theme) -> u16;

    /// Render into the given buffer area
    ///
    /// When `clip` is Some, the block is partially visible and should:
    /// - Skip top border if clip.clip_top > 0
    /// - Skip bottom border if clip.clip_bottom > 0
    /// - Adjust content rendering for the visible portion
    fn render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        focused: bool,
        clip: Option<ClipContext>,
    );

    /// Handle input events
    ///
    /// When `clip` is Some, translate screen coordinates to block-internal:
    /// internal_y = (screen_y - area.y) + clip.clip_top
    fn handle_event(
        &mut self,
        event: &Event,
        area: Rect,
        clip: Option<ClipContext>,
    ) -> EventResult {
        let _ = (event, area, clip);
        EventResult::Ignored
    }

    /// Get copyable text content
    fn get_text_content(&self) -> Option<String> {
        None
    }

    /// Update animation state, returns true if needs redraw
    fn tick(&mut self) -> bool {
        false
    }

    /// Is this block currently streaming/loading?
    fn is_streaming(&self) -> bool {
        false
    }
}

// Re-exports
pub use bash::BashBlock;
pub use build::BuildBlock;
pub use edit::{DiffMode, EditBlock};
pub use explore::ExploreBlock;
pub use read::ReadBlock;
pub use terminal_pane::TerminalPane;
pub use thinking::ThinkingBlock;
pub use tool_result::ToolResultBlock;
pub use web_search::WebSearchBlock;
pub use write::WriteBlock;
