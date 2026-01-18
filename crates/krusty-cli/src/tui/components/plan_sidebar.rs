//! Plan sidebar component
//!
//! Renders a collapsible sidebar showing the current plan's phases and tasks.
//! Uses caching to avoid rebuilding content every frame.

use std::borrow::Cow;
use std::hash::{Hash, Hasher};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Result of rendering the plan sidebar
pub struct PlanSidebarRenderResult {
    /// Scrollbar area (if scrolling is needed)
    pub scrollbar_area: Option<Rect>,
}

use super::scrollbars::render_scrollbar;
use crate::plan::PlanFile;
use crate::tui::themes::Theme;

/// Sidebar width when fully expanded
pub const SIDEBAR_WIDTH: u16 = 76;

/// Minimum terminal width to show sidebar
pub const MIN_TERMINAL_WIDTH: u16 = 140;

/// Plan sidebar state with content caching
#[derive(Debug, Clone, Default)]
pub struct PlanSidebarState {
    /// Whether sidebar is visible
    pub visible: bool,
    /// Current animated width (0 to SIDEBAR_WIDTH)
    pub current_width: u16,
    /// Target width (0 or SIDEBAR_WIDTH)
    pub target_width: u16,
    /// Scroll offset for content
    pub scroll_offset: usize,
    /// Total content lines (calculated during render)
    pub total_lines: usize,
    /// Pending plan clear after collapse animation completes
    pending_clear: bool,

    // === Caching fields ===
    /// Cached rendered lines (avoids rebuilding every frame)
    cached_lines: Vec<Line<'static>>,
    /// Hash of plan content when cache was built
    cached_plan_hash: u64,
    /// Width when cache was built
    cached_width: u16,
}

impl PlanSidebarState {
    /// Toggle sidebar visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.target_width = if self.visible { SIDEBAR_WIDTH } else { 0 };
        // Reset scroll when toggling
        if !self.visible {
            self.scroll_offset = 0;
        }
    }

    /// Start graceful collapse animation (for plan completion)
    /// The plan should be cleared after animation completes via should_clear_plan()
    pub fn start_collapse(&mut self) {
        self.target_width = 0;
        self.pending_clear = true;
    }

    /// Check if plan should be cleared (collapse animation complete)
    /// Returns true once and resets the pending flag
    pub fn should_clear_plan(&mut self) -> bool {
        if self.pending_clear && self.current_width == 0 {
            self.pending_clear = false;
            self.visible = false;
            self.scroll_offset = 0;
            true
        } else {
            false
        }
    }

    /// Reset sidebar to initial state
    pub fn reset(&mut self) {
        self.visible = false;
        self.current_width = 0;
        self.target_width = 0;
        self.scroll_offset = 0;
        self.total_lines = 0;
        self.pending_clear = false;
        // Clear cache
        self.cached_lines.clear();
        self.cached_plan_hash = 0;
        self.cached_width = 0;
    }

    /// Animate width towards target
    /// Returns true if animation is still in progress
    pub fn tick(&mut self) -> bool {
        if self.current_width == self.target_width {
            return false;
        }

        // Adaptive animation speed: faster when far from target
        let remaining = (self.target_width as i16 - self.current_width as i16).unsigned_abs();
        let step = (remaining / 5).clamp(2, 8);

        if self.current_width < self.target_width {
            self.current_width = (self.current_width + step).min(self.target_width);
        } else {
            self.current_width = self.current_width.saturating_sub(step);
            if self.current_width < step {
                self.current_width = self.target_width;
            }
        }

        self.current_width != self.target_width
    }

    /// Get current width for layout calculations
    pub fn width(&self) -> u16 {
        self.current_width
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self, visible_height: usize) {
        let max_offset = self.total_lines.saturating_sub(visible_height);
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
    }

    /// Scroll up by a page
    pub fn page_up(&mut self, visible_height: usize) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_sub(visible_height.saturating_sub(2));
    }

    /// Scroll down by a page
    pub fn page_down(&mut self, visible_height: usize) {
        let max_offset = self.total_lines.saturating_sub(visible_height);
        self.scroll_offset =
            (self.scroll_offset + visible_height.saturating_sub(2)).min(max_offset);
    }

    /// Handle scrollbar click - jump to position
    pub fn handle_scrollbar_click(&mut self, click_y: u16, area: Rect) {
        if self.total_lines == 0 {
            return;
        }

        let relative_y = click_y.saturating_sub(area.y) as f32;
        let height = area.height as f32;
        let visible_height = area.height as usize;
        let max_offset = self.total_lines.saturating_sub(visible_height);

        if max_offset == 0 {
            return;
        }

        let new_offset = ((relative_y / height) * max_offset as f32).round() as usize;
        self.scroll_offset = new_offset.min(max_offset);
    }
}

/// Render the plan sidebar
/// Returns render result with scrollbar area for hit testing
/// Uses caching to avoid rebuilding content every frame
pub fn render_plan_sidebar(
    buf: &mut Buffer,
    area: Rect,
    plan: &PlanFile,
    theme: &Theme,
    state: &mut PlanSidebarState,
) -> PlanSidebarRenderResult {
    if area.width < 10 || area.height < 5 {
        return PlanSidebarRenderResult {
            scrollbar_area: None,
        };
    }

    // Draw clean border (no title)
    let block = Block::default()
        .borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_color))
        .style(Style::default().bg(theme.bg_color));

    let inner = block.inner(area);
    block.render(area, buf);

    if inner.width < 5 || inner.height < 3 {
        return PlanSidebarRenderResult {
            scrollbar_area: None,
        };
    }

    // Calculate total lines: phase headers + tasks + separators between phases
    let total_lines = plan.phases.len()
        + plan.phases.iter().map(|p| p.tasks.len()).sum::<usize>()
        + plan.phases.len().saturating_sub(1);
    state.total_lines = total_lines;

    // Clamp scroll offset
    let visible_height = inner.height as usize;
    let max_offset = total_lines.saturating_sub(visible_height);
    if state.scroll_offset > max_offset {
        state.scroll_offset = max_offset;
    }

    // Reserve space for scrollbar if needed
    let content_width = if total_lines > visible_height {
        inner.width.saturating_sub(2)
    } else {
        inner.width
    };

    // Check if we need to rebuild the cache
    let plan_hash = hash_plan(plan);
    let cache_valid = state.cached_plan_hash == plan_hash && state.cached_width == content_width;

    if !cache_valid {
        // Rebuild cached lines
        state.cached_lines.clear();

        for (i, phase) in plan.phases.iter().enumerate() {
            // Phase header
            let phase_title = format!("Phase {}: {}", phase.number, phase.name);
            let phase_title = truncate_str(&phase_title, content_width as usize - 1);
            state.cached_lines.push(Line::from(vec![Span::styled(
                phase_title.into_owned(),
                Style::default()
                    .fg(theme.accent_color)
                    .add_modifier(Modifier::BOLD),
            )]));

            // Tasks
            for task in &phase.tasks {
                let checkbox = if task.completed { "✓" } else { "○" };
                let checkbox_color = if task.completed {
                    theme.success_color
                } else {
                    theme.dim_color
                };

                let desc = truncate_str(&task.description, content_width as usize - 4);
                let task_style = if task.completed {
                    Style::default().fg(theme.dim_color)
                } else {
                    Style::default().fg(theme.text_color)
                };

                state.cached_lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(checkbox, Style::default().fg(checkbox_color)),
                    Span::raw(" "),
                    Span::styled(desc.into_owned(), task_style),
                ]));
            }

            // Space between phases (not after last)
            if i < plan.phases.len() - 1 {
                state.cached_lines.push(Line::from(""));
            }
        }

        state.cached_plan_hash = plan_hash;
        state.cached_width = content_width;
    }

    // Render visible lines from cache using slice
    let start = state.scroll_offset;
    let end = (start + visible_height).min(state.cached_lines.len());
    let mut y = inner.y;

    for line in &state.cached_lines[start..end] {
        render_line(buf, inner.x, y, content_width, line);
        y += 1;
    }

    // Render scrollbar if needed
    let scrollbar_area = if total_lines > visible_height {
        let scrollbar_rect = Rect::new(inner.x + inner.width - 1, inner.y, 1, inner.height);
        render_scrollbar(
            buf,
            scrollbar_rect,
            state.scroll_offset,
            total_lines,
            visible_height,
            theme.accent_color,
            theme.scrollbar_bg_color,
        );
        Some(scrollbar_rect)
    } else {
        None
    };

    PlanSidebarRenderResult { scrollbar_area }
}

/// Render a line directly to the buffer without cloning
fn render_line(buf: &mut Buffer, x: u16, y: u16, width: u16, line: &Line) {
    let mut cx = x;
    let max_x = x + width;

    for span in &line.spans {
        for ch in span.content.chars() {
            if cx >= max_x {
                return;
            }
            let char_width = ch.width().unwrap_or(1) as u16;
            if cx + char_width > max_x {
                return;
            }
            if let Some(cell) = buf.cell_mut((cx, y)) {
                cell.set_char(ch);
                cell.set_style(span.style);
            }
            cx += char_width;
        }
    }
}

/// Compute a hash of the plan content for cache invalidation
fn hash_plan(plan: &PlanFile) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    // Hash phase count and each phase's content
    plan.phases.len().hash(&mut hasher);
    for phase in &plan.phases {
        phase.number.hash(&mut hasher);
        phase.name.hash(&mut hasher);
        phase.tasks.len().hash(&mut hasher);
        for task in &phase.tasks {
            task.description.hash(&mut hasher);
            task.completed.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Truncate a string to fit within max_width display columns, adding ellipsis if needed
/// Uses Cow to avoid allocation when no truncation is needed
fn truncate_str(s: &str, max_width: usize) -> Cow<'_, str> {
    let current_width = s.width();
    if current_width <= max_width {
        return Cow::Borrowed(s);
    }

    if max_width <= 1 {
        // Not enough space for ellipsis, just take what fits
        let mut result = String::new();
        let mut width = 0;
        for c in s.chars() {
            let cw = c.width().unwrap_or(1);
            if width + cw > max_width {
                break;
            }
            result.push(c);
            width += cw;
        }
        return Cow::Owned(result);
    }

    // Reserve 1 column for ellipsis (…)
    let target_width = max_width - 1;
    let mut result = String::new();
    let mut width = 0;

    for c in s.chars() {
        let cw = c.width().unwrap_or(1);
        if width + cw > target_width {
            break;
        }
        result.push(c);
        width += cw;
    }

    result.push('…');
    Cow::Owned(result)
}
