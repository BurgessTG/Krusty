//! Popup State Component
//!
//! Groups all popup controller states into a single component.

use crate::tui::popups::{
    auth::AuthPopup, file_preview::FilePreviewPopup, help::HelpPopup,
    hooks::HooksPopup, lsp_browser::LspBrowserPopup, lsp_install::LspInstallPopup,
    mcp_browser::McpBrowserPopup, model_select::ModelSelectPopup, pinch::PinchPopup,
    process_list::ProcessListPopup, session_list::SessionListPopup,
    skills_browser::SkillsBrowserPopup, theme_select::ThemeSelectPopup,
};

/// All popup controller states grouped together
pub struct PopupState {
    pub help: HelpPopup,
    pub theme: ThemeSelectPopup,
    pub model: ModelSelectPopup,
    pub session: SessionListPopup,
    pub auth: AuthPopup,
    pub lsp: LspBrowserPopup,
    pub lsp_install: LspInstallPopup,
    pub mcp: McpBrowserPopup,
    pub process: ProcessListPopup,
    pub pinch: PinchPopup,
    pub file_preview: FilePreviewPopup,
    pub skills: SkillsBrowserPopup,
    pub hooks: HooksPopup,
}

impl PopupState {
    pub fn new() -> Self {
        let mut file_preview = FilePreviewPopup::new();
        file_preview.init_graphics();

        Self {
            help: HelpPopup::new(),
            theme: ThemeSelectPopup::new(),
            model: ModelSelectPopup::new(),
            session: SessionListPopup::new(),
            auth: AuthPopup::new(),
            lsp: LspBrowserPopup::new(),
            lsp_install: LspInstallPopup::new(),
            mcp: McpBrowserPopup::new(),
            process: ProcessListPopup::new(),
            pinch: PinchPopup::new(),
            file_preview,
            skills: SkillsBrowserPopup::new(),
            hooks: HooksPopup::new(),
        }
    }
}

impl Default for PopupState {
    fn default() -> Self {
        Self::new()
    }
}
