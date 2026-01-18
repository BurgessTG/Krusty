//! Auto-update handlers
//!
//! Background update checking and downloading from GitHub releases.

use crate::tui::app::App;
use crate::tui::components::Toast;
use krusty_core::updater::{apply_update, check_for_updates, download_update, UpdateStatus};
use tokio::sync::mpsc;

impl App {
    /// Check for persisted update on startup (binary ready to apply)
    pub fn check_persisted_update(&mut self) {
        let temp_binary = std::env::temp_dir().join("krusty-update");
        if temp_binary.exists() {
            // There's a pending update - try to apply it
            match apply_update(&temp_binary) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&temp_binary);
                    self.show_toast(Toast::success("Update applied! Restart to use new version.").persistent());
                }
                Err(e) => {
                    tracing::warn!("Failed to apply persisted update: {}", e);
                    let _ = std::fs::remove_file(&temp_binary);
                }
            }
        }
    }

    /// Start background update check
    pub fn start_update_check(&mut self) {
        // Don't start if already checking
        if self.channels.update_status.is_some() {
            return;
        }

        // Don't check if we already have an update ready
        if matches!(self.update_status, Some(UpdateStatus::Ready { .. })) {
            self.show_toast(Toast::info("Update ready - restart to apply"));
            return;
        }

        // Create channel for status updates
        let (tx, rx) = mpsc::unbounded_channel();
        self.channels.update_status = Some(rx);

        // Spawn background task to check for updates
        tokio::spawn(async move {
            let _ = tx.send(UpdateStatus::Checking);

            match check_for_updates().await {
                Ok(Some(info)) => {
                    let _ = tx.send(UpdateStatus::Available(info.clone()));

                    // Auto-download the update
                    match download_update(&info, tx.clone()).await {
                        Ok(_) => {
                            // Ready status already sent by download_update
                        }
                        Err(e) => {
                            let _ = tx.send(UpdateStatus::Error(e.to_string()));
                        }
                    }
                }
                Ok(None) => {
                    let _ = tx.send(UpdateStatus::UpToDate);
                }
                Err(e) => {
                    tracing::debug!("Update check failed: {}", e);
                    // Don't show error toast for network issues - silent fail
                }
            }
        });
    }

    /// Manually trigger update download (if update is available)
    pub fn download_available_update(&mut self) {
        let info = match &self.update_status {
            Some(UpdateStatus::Available(info)) => info.clone(),
            _ => {
                self.show_toast(Toast::info("No update available"));
                return;
            }
        };

        // Create channel for status updates
        let (tx, rx) = mpsc::unbounded_channel();
        self.channels.update_status = Some(rx);

        tokio::spawn(async move {
            match download_update(&info, tx.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    let _ = tx.send(UpdateStatus::Error(e.to_string()));
                }
            }
        });
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
                UpdateStatus::Checking => {
                    // Silent - don't spam user
                }
                UpdateStatus::UpToDate => {
                    // Silent on startup - only show if manually triggered
                    self.update_status = None;
                    clear_channel = true;
                }
                UpdateStatus::Available(info) => {
                    self.show_toast(Toast::info(format!(
                        "Update available: v{}",
                        info.new_version
                    )));
                }
                UpdateStatus::Downloading { progress } => {
                    tracing::debug!("Update progress: {}", progress);
                }
                UpdateStatus::Ready { version, new_binary } => {
                    // Auto-apply the update
                    match apply_update(new_binary) {
                        Ok(()) => {
                            self.show_toast(
                                Toast::success(format!("Updated to v{} - restart to apply", version))
                                    .persistent(),
                            );
                        }
                        Err(e) => {
                            self.show_toast(Toast::info(format!("Update ready but failed to apply: {}", e)));
                        }
                    }
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
