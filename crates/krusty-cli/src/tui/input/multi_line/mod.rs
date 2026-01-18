//! Multi-line input handler with proper text wrapping and cursor management

use crossterm::event::{KeyCode, KeyModifiers};

mod editor;
mod renderer;
mod viewport;
mod wrapper;

pub use editor::InputAction;

/// Multi-line input handler with proper text wrapping and cursor management
pub struct MultiLineInput {
    /// The actual text content
    pub(crate) content: String,
    /// Current cursor position in the content (byte offset)
    pub(crate) cursor_position: usize,
    /// Visual cursor position (line, column in bytes)
    pub(crate) cursor_visual: (usize, usize),
    /// Width of the input area for wrapping
    pub(crate) width: u16,
    /// Viewport offset for scrolling
    pub(crate) viewport_offset: usize,
    /// Maximum visible lines
    pub(crate) max_visible_lines: u16,
}

impl MultiLineInput {
    pub fn new(max_visible_lines: u16) -> Self {
        Self {
            content: String::new(),
            cursor_position: 0,
            cursor_visual: (0, 0),
            width: 80,
            viewport_offset: 0,
            max_visible_lines,
        }
    }

    pub fn set_width(&mut self, width: u16) {
        // Account for borders + padding + scrollbar
        self.width = width.saturating_sub(4).max(10);
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor_position = 0;
        self.cursor_visual = (0, 0);
        self.viewport_offset = 0;
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn set_max_visible_lines(&mut self, lines: u16) {
        if self.max_visible_lines != lines {
            self.max_visible_lines = lines;
            self.ensure_cursor_visible();
        }
    }

    // Editor methods
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> InputAction {
        self.handle_key_impl(code, modifiers)
    }

    pub fn insert_char(&mut self, ch: char) {
        self.insert_char_impl(ch)
    }

    pub fn insert_text(&mut self, text: &str) {
        self.insert_text_impl(text)
    }

    // Wrapper methods
    pub fn get_wrapped_lines(&self) -> Vec<String> {
        self.get_wrapped_lines_impl()
    }

    pub fn get_wrapped_lines_count(&self) -> usize {
        self.get_wrapped_lines().len()
    }

    // Viewport methods
    pub fn handle_click(&mut self, x: u16, y: u16) {
        self.handle_click_impl(x, y)
    }

    pub fn scroll_up(&mut self) {
        self.scroll_up_impl()
    }

    pub fn scroll_down(&mut self) {
        self.scroll_down_impl()
    }

    pub fn get_max_visible_lines(&self) -> u16 {
        self.max_visible_lines
    }

    pub fn get_viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    pub fn set_viewport_offset(&mut self, offset: usize) {
        let total_lines = self.get_wrapped_lines().len();
        let max_offset = total_lines.saturating_sub(self.max_visible_lines as usize);
        self.viewport_offset = offset.min(max_offset);
    }

    /// Get file reference at click position (relative to input area)
    /// Returns (byte_start, byte_end, path) if click is on a file reference
    pub fn get_file_ref_at_click(&self, x: u16, y: u16) -> Option<(usize, usize, std::path::PathBuf)> {
        use regex::Regex;
        use std::sync::LazyLock;

        // Pattern for bracketed file paths
        static BRACKET_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\[([^\]]+\.(png|jpe?g|gif|webp|pdf))\]").unwrap());

        // Convert click to byte position
        let content_x = x.saturating_sub(1) as usize;
        let content_y = y.saturating_sub(1) as usize;
        let clicked_line = self.viewport_offset + content_y;

        let lines = self.get_wrapped_lines();
        if clicked_line >= lines.len() {
            return None;
        }

        // Calculate byte offset for this line in original content
        let mut byte_offset = 0usize;
        let mut current_line = 0usize;

        for ch in self.content.chars() {
            if current_line == clicked_line {
                break;
            }
            byte_offset += ch.len_utf8();
            // Simplified: just count newlines for now
            if ch == '\n' {
                current_line += 1;
            }
        }

        // Find bracketed file refs in content
        for caps in BRACKET_PATTERN.captures_iter(&self.content) {
            let m = caps.get(0)?;
            let path_str = caps.get(1)?.as_str();

            // Check if click is within this match
            let start = m.start();
            let end = m.end();

            // Simple check: if the match is on the same line region
            if start <= byte_offset + content_x && byte_offset + content_x < end {
                let path = std::path::PathBuf::from(path_str);
                if path.exists() {
                    return Some((start, end, path));
                }
            }
        }

        None
    }

    /// Get all file reference ranges in content for styling
    /// Returns vec of (byte_start, byte_end) for each file reference
    pub fn get_file_ref_ranges(&self) -> Vec<(usize, usize)> {
        use regex::Regex;
        use std::sync::LazyLock;

        static BRACKET_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\[([^\]]+\.(png|jpe?g|gif|webp|pdf))\]").unwrap());

        BRACKET_PATTERN
            .find_iter(&self.content)
            .map(|m| (m.start(), m.end()))
            .collect()
    }
}
