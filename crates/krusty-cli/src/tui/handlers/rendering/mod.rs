//! UI rendering coordinator
//!
//! Main entry point that dispatches to specialized render modules.

mod messages;
mod views;

use ratatui::{style::Style, widgets::Block, Frame};

use crate::tui::app::{App, Popup, View};
use crate::tui::components::render_toasts;

impl App {
    /// Main UI rendering dispatcher
    pub fn ui(&mut self, f: &mut Frame) {
        let bg = Block::default().style(Style::default().bg(self.theme.bg_color));
        f.render_widget(bg, f.area());

        match self.view {
            View::StartMenu => self.render_start_menu(f),
            View::Chat => self.render_chat(f),
        }

        // Render popup on top
        match &self.popup {
            Popup::None => {}
            Popup::Help => self.popups.help.render(f, &self.theme),
            Popup::ThemeSelect => self.popups.theme.render(f, &self.theme, &self.theme_name),
            Popup::ModelSelect => self.popups.model.render(
                f,
                &self.theme,
                &self.current_model,
                self.context_tokens_used,
            ),
            Popup::SessionList => self.popups.session.render(f, &self.theme),
            Popup::Auth => self.popups.auth.render(f, &self.theme),
            Popup::LspBrowser => self.popups.lsp.render(f, &self.theme),
            Popup::LspInstall => self.popups.lsp_install.render(f, &self.theme),
            Popup::ProcessList => self.popups.process.render(f, &self.theme),
            Popup::Pinch => self.popups.pinch.render(f, &self.theme),
            Popup::FilePreview => self.popups.file_preview.render(f, &self.theme),
            Popup::SkillsBrowser => self.popups.skills.render(f, &self.theme),
            Popup::McpBrowser => self.popups.mcp.render(f, &self.theme),
            Popup::Hooks => self.popups.hooks.render(f, &self.theme),
        }

        // Render toasts on top of everything
        let area = f.area();
        render_toasts(f.buffer_mut(), area, &self.toasts, &self.theme);
    }
}
