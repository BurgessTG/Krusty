//! Auto-update handlers
//!
//! Background update checking and building.
//! NOTE: Currently disabled - update infrastructure incomplete.

use krusty_core::updater::UpdateStatus;
use crate::tui::app::App;
use crate::tui::components::Toast;

impl App {
    /// Check for persisted update on startup
    pub fn check_persisted_update(&mut self) {
        // Update infrastructure incomplete
    }

    /// Start background update check
    pub fn start_update_check(&mut self) {
        // Don't start if already checking or no repo path
        if self.channels.update_status.is_some() || self.update_repo_path.is_none() {
            return;
        }

        // Don't check if we already have an update ready
        if matches!(self.update_status, Some(UpdateStatus::Ready { .. })) {
            self.show_toast(Toast::info("Update already ready - restart to apply"));
            return;
        }

        // TODO: Re-enable when update infrastructure is complete
        self.show_toast(Toast::info("Update checking not yet available"));
    }

    /// Poll update status channel and show toasts
    pub fn poll_update_status(&mut self) {
        let statuses: Vec<UpdateStatus> = if let Some(ref mut rx) = self.channels.update_status {
            let mut collected = Vec::new();
            while let Ok(status) = rx.try_recv() {
                collected.push(status);
            }
            collected
        } else {
            return;
        };

        let mut clear_channel = false;

        for status in statuses {
            match &status {
                UpdateStatus::Checking => {}
                UpdateStatus::UpToDate => {
                    self.show_toast(Toast::success("Up to date!"));
                    self.update_status = None;
                    clear_channel = true;
                }
                UpdateStatus::Available(info) => {
                    self.show_toast(Toast::info(format!(
                        "Update available: {} commits behind",
                        info.commits_behind
                    )));
                }
                UpdateStatus::Building { progress } => {
                    tracing::debug!("Update build progress: {}", progress);
                }
                UpdateStatus::Ready { new_binary: _ } => {
                    self.show_toast(Toast::success("Update ready! Restart to apply.").persistent());
                    clear_channel = true;
                }
                UpdateStatus::Error(e) => {
                    self.show_toast(Toast::info(format!("Update failed: {}", e)));
                    self.update_status = None;
                    clear_channel = true;
                }
            }
            self.update_status = Some(status);
        }

        if clear_channel {
            self.channels.update_status = None;
        }
    }
}
