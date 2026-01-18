//! Selection State - Text selection and scrollbar drag management
//!
//! Handles mouse-based text selection across messages and input areas,
//! as well as scrollbar drag tracking for various block types.

use crate::tui::blocks::BlockType;

/// Scrollbar drag state for block scrollbars
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockScrollbarDrag {
    pub block_type: BlockType,
    pub index: usize,
    pub scrollbar_y: u16,
    pub scrollbar_height: u16,
    pub total_lines: u16,
    pub visible_lines: u16,
}

impl BlockScrollbarDrag {
    /// Calculate scroll offset from current mouse y position
    pub fn calculate_offset(&self, y: u16) -> Option<u16> {
        let max_scroll = self.total_lines.saturating_sub(self.visible_lines);
        if self.scrollbar_height == 0 || max_scroll == 0 {
            return None;
        }
        let relative_y = y.saturating_sub(self.scrollbar_y);
        let ratio = (relative_y as f32 / self.scrollbar_height as f32).clamp(0.0, 1.0);
        Some((ratio * max_scroll as f32).round() as u16)
    }
}

/// Which scrollbar is being dragged
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragTarget {
    Input,
    Messages,
    /// Plan sidebar scrollbar
    PlanSidebar,
    /// Block scrollbar (consolidated for all block types)
    Block(BlockScrollbarDrag),
}

/// Edge scroll direction during selection drag
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeScrollDirection {
    Up,
    Down,
}

/// Edge scroll state for continuous scrolling while holding at edge
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeScrollState {
    pub direction: Option<EdgeScrollDirection>,
    pub area: SelectionArea,
    pub last_x: u16,
}

/// Which area text selection is happening in
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SelectionArea {
    #[default]
    None,
    Messages,
    Input,
}

/// Text selection state - position as (line, column)
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub start: Option<(usize, usize)>,
    pub end: Option<(usize, usize)>,
    pub is_selecting: bool,
    pub area: SelectionArea,
}

impl SelectionState {
    /// Get normalized selection (start always before end)
    pub fn normalized(&self) -> Option<((usize, usize), (usize, usize))> {
        let (start, end) = (self.start?, self.end?);
        Some(if start <= end { (start, end) } else { (end, start) })
    }

    /// Check if selection is non-empty
    pub fn has_selection(&self) -> bool {
        matches!((self.start, self.end), (Some(s), Some(e)) if s != e)
    }

    /// Clear selection state
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
        self.is_selecting = false;
        self.area = SelectionArea::None;
    }
}
