//! Scrollbar handling
//!
//! Handles scrollbar click and drag operations for messages, input, and block scrollbars.

use crate::tui::app::App;
use crate::tui::blocks::BlockType;
use crate::tui::state::DragTarget;

impl App {
    /// Handle messages scrollbar click - jump to position
    pub fn handle_messages_scrollbar_click(&mut self, click_y: u16, area: ratatui::layout::Rect) {
        if !self.scroll.needs_scrollbar() {
            return;
        }

        // Calculate what offset corresponds to this y position
        let relative_y = click_y.saturating_sub(area.y) as f32;
        let height = area.height as f32;

        let new_offset = ((relative_y / height) * self.scroll.max_scroll as f32).round() as usize;
        self.scroll.scroll_to_line(new_offset);
    }

    /// Handle input scrollbar click - jump to position
    pub fn handle_input_scrollbar_click(&mut self, click_y: u16, area: ratatui::layout::Rect) {
        let total_lines = self.input.get_wrapped_lines_count();
        let visible_lines = self.input.get_max_visible_lines() as usize;

        if total_lines <= visible_lines {
            return;
        }

        // Calculate what offset corresponds to this y position
        let relative_y = click_y.saturating_sub(area.y) as f32;
        let height = area.height as f32;
        let max_offset = total_lines.saturating_sub(visible_lines);

        let new_offset = ((relative_y / height) * max_offset as f32).round() as usize;
        self.input.set_viewport_offset(new_offset.min(max_offset));
    }

    /// Handle plan sidebar scrollbar click - jump to position
    pub fn handle_plan_sidebar_scrollbar_click(
        &mut self,
        click_y: u16,
        area: ratatui::layout::Rect,
    ) {
        self.plan_sidebar.handle_scrollbar_click(click_y, area);
    }

    /// Handle scrollbar drag - routes to appropriate scrollbar based on drag target
    ///
    /// Returns true if a scrollbar drag was handled.
    pub fn handle_scrollbar_drag(&mut self, y: u16) -> bool {
        match self.layout.dragging_scrollbar {
            Some(DragTarget::Messages) => {
                if let Some(area) = self.layout.messages_scrollbar_area {
                    self.handle_messages_scrollbar_click(y, area);
                }
                true
            }
            Some(DragTarget::Input) => {
                if let Some(area) = self.layout.input_scrollbar_area {
                    self.handle_input_scrollbar_click(y, area);
                }
                true
            }
            Some(DragTarget::PlanSidebar) => {
                if let Some(area) = self.layout.plan_sidebar_scrollbar_area {
                    self.handle_plan_sidebar_scrollbar_click(y, area);
                }
                true
            }
            Some(DragTarget::Block(drag)) => {
                if let Some(offset) = drag.calculate_offset(y) {
                    match drag.block_type {
                        BlockType::Thinking => {
                            if let Some(block) = self.blocks.thinking.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::ToolResult => {
                            if let Some(block) = self.blocks.tool_result.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::Bash => {
                            if let Some(block) = self.blocks.bash.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::Read => {
                            if let Some(block) = self.blocks.read.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::Edit => {
                            if let Some(block) = self.blocks.edit.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::Write => {
                            if let Some(block) = self.blocks.write.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::WebSearch => {
                            if let Some(block) = self.blocks.web_search.get_mut(drag.index) {
                                block.set_scroll_offset(offset);
                            }
                        }
                        BlockType::TerminalPane => {
                            // Terminal panes don't use this scrollbar system
                        }
                        BlockType::Explore => {
                            // Explore blocks don't use this scrollbar system
                        }
                        BlockType::Build => {
                            // Build blocks don't use this scrollbar system
                        }
                    }
                }
                true
            }
            None => false,
        }
    }
}
