//! Auto-updater module for Krusty
//!
//! Checks for updates from git, builds in background, and prepares for restart.

mod checker;

pub use checker::{check_for_updates, build_update, detect_repo_path, UpdateStatus, UpdateInfo};
