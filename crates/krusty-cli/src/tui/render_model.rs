//! Render model data for TUI rendering boundaries.
//!
//! Provides a lightweight snapshot of top-level render state for the UI layer.
//!
//! NOTE: This module is part of an in-progress TUI refactor. The pattern is
//! ready but not yet integrated into all render paths.

use std::sync::Arc;

use crate::tui::app::{App, Popup, View, WorkMode};
use crate::tui::components::ToastQueue;
use crate::tui::themes::Theme;

/// Read-only view of top-level render state.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct RenderModel<'a> {
    pub view: &'a View,
    pub popup: &'a Popup,
    pub theme: &'a Arc<Theme>,
    pub theme_name: &'a str,
    pub work_mode: WorkMode,
    pub toasts: &'a ToastQueue,
}

impl<'a> RenderModel<'a> {
    #[allow(dead_code)]
    pub fn from_app(app: &'a App) -> Self {
        Self {
            view: &app.view,
            popup: &app.popup,
            theme: &app.theme,
            theme_name: &app.theme_name,
            work_mode: app.work_mode,
            toasts: &app.toasts,
        }
    }
}

impl App {
    /// Build a read-only render model snapshot for the UI layer.
    #[allow(dead_code)]
    pub fn render_model(&self) -> RenderModel<'_> {
        RenderModel::from_app(self)
    }
}
