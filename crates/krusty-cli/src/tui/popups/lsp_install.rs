//! LSP install prompt popup
//!
//! Shown when a file is opened/edited without an LSP server available.
//! Offers to install the suggested LSP server.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::common::{center_rect, popup_block, popup_title, render_popup_background, PopupSize};
use crate::lsp::manager::{LspSuggestion, MissingLspInfo};
use crate::tui::themes::Theme;

/// LSP installation prompt state
pub struct LspInstallPopup {
    /// Current missing LSP info (if any)
    pub info: Option<MissingLspInfo>,
    /// Whether installation is in progress
    pub installing: bool,
    /// Installation progress message
    pub progress_msg: Option<String>,
    /// Whether an error occurred (for UI state)
    pub has_error: bool,
}

impl Default for LspInstallPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl LspInstallPopup {
    pub fn new() -> Self {
        Self {
            info: None,
            installing: false,
            progress_msg: None,
            has_error: false,
        }
    }

    /// Set the missing LSP info to prompt about
    pub fn set(&mut self, info: MissingLspInfo) {
        self.info = Some(info);
        self.installing = false;
        self.progress_msg = None;
        self.has_error = false;
    }

    /// Clear the popup state
    pub fn clear(&mut self) {
        self.info = None;
        self.installing = false;
        self.progress_msg = None;
        self.has_error = false;
    }

    /// Get the current info if any
    pub fn get_info(&self) -> Option<&MissingLspInfo> {
        self.info.as_ref()
    }

    /// Mark installation as started
    pub fn start_install(&mut self) {
        self.installing = true;
        self.progress_msg = Some("Downloading...".to_string());
    }

    /// Update progress message
    pub fn set_progress(&mut self, msg: &str) {
        self.progress_msg = Some(msg.to_string());
    }

    /// Set error state (allows dismissal)
    pub fn set_error(&mut self, msg: &str) {
        self.installing = false;
        self.has_error = true;
        self.progress_msg = Some(msg.to_string());
    }

    pub fn render(&self, f: &mut Frame, theme: &Theme) {
        let info = match &self.info {
            Some(i) => i,
            None => return,
        };

        let (w, h) = PopupSize::Small.dimensions();
        let area = center_rect(w, h.min(12), f.area());
        render_popup_background(f, area, theme);

        let block = popup_block(theme);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(4),    // Content
                Constraint::Length(2), // Footer
            ])
            .split(inner);

        // Title
        let title = format!("No LSP for .{} files", info.extension);
        let title_lines = popup_title(&title, theme);
        let title_widget = Paragraph::new(title_lines).alignment(Alignment::Center);
        f.render_widget(title_widget, chunks[0]);

        // Content
        let mut lines: Vec<Line> = Vec::new();

        if self.installing {
            // Show progress
            let msg = self
                .progress_msg
                .as_deref()
                .unwrap_or("Installing...");
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                format!("  {} ", msg),
                Style::default().fg(theme.accent_color),
            )]));
        } else {
            // Show suggestion
            match &info.suggested {
                LspSuggestion::Builtin(builtin) => {
                    lines.push(Line::from(vec![
                        Span::styled("  Install ", Style::default().fg(theme.text_color)),
                        Span::styled(
                            builtin.binary,
                            Style::default()
                                .fg(theme.accent_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("?", Style::default().fg(theme.text_color)),
                    ]));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "  (built-in, auto-downloads)",
                        Style::default().fg(theme.dim_color),
                    )]));
                }
                LspSuggestion::Extension(name) => {
                    lines.push(Line::from(vec![
                        Span::styled("  Install ", Style::default().fg(theme.text_color)),
                        Span::styled(
                            name,
                            Style::default()
                                .fg(theme.accent_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" extension?", Style::default().fg(theme.text_color)),
                    ]));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "  (from Zed marketplace)",
                        Style::default().fg(theme.dim_color),
                    )]));
                }
                LspSuggestion::None => {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  No LSP available for {} files", info.language),
                        Style::default().fg(theme.dim_color),
                    )]));
                }
            }
        }

        let content = Paragraph::new(lines).style(Style::default().bg(theme.bg_color));
        f.render_widget(content, chunks[1]);

        // Footer - show appropriate options based on state
        let footer = if self.installing {
            Paragraph::new(Line::from(vec![Span::styled(
                "Please wait...",
                Style::default().fg(theme.dim_color),
            )]))
        } else if self.has_error {
            // Error occurred - show dismissal option
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": close", Style::default().fg(theme.text_color)),
            ]))
        } else if matches!(info.suggested, LspSuggestion::None) {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": close", Style::default().fg(theme.text_color)),
            ]))
        } else {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "Y",
                    Style::default()
                        .fg(theme.success_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": install  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "N",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": skip  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "A",
                    Style::default()
                        .fg(theme.dim_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": always skip", Style::default().fg(theme.text_color)),
            ]))
        };
        f.render_widget(footer.alignment(Alignment::Center), chunks[2]);
    }
}
