//! Update checker and builder

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;
use tokio::sync::mpsc;

/// Update status
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateStatus {
    /// Currently checking for updates
    Checking,
    /// No updates available
    UpToDate,
    /// Update available, not yet building
    Available(UpdateInfo),
    /// Building update in background
    Building { progress: String },
    /// Build complete, ready to apply
    Ready { new_binary: PathBuf },
    /// Error occurred
    Error(String),
}

/// Information about an available update
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateInfo {
    /// Current commit hash (short)
    pub current_commit: String,
    /// New commit hash (short)
    pub new_commit: String,
    /// Number of commits behind
    pub commits_behind: usize,
    /// Latest commit message
    pub commit_message: String,
}

/// Check for updates by comparing local HEAD with origin/main
/// 
/// Returns UpdateInfo if updates are available, None if up to date.
pub fn check_for_updates(repo_path: &PathBuf) -> Result<Option<UpdateInfo>> {
    // Fetch from origin (quiet)
    let fetch_status = Command::new("git")
        .args(["fetch", "origin", "main", "--quiet"])
        .current_dir(repo_path)
        .status()?;

    if !fetch_status.success() {
        return Err(anyhow!("Failed to fetch from origin"));
    }

    // Get current HEAD
    let current = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()?;
    let current_commit = String::from_utf8_lossy(&current.stdout).trim().to_string();

    // Get origin/main HEAD
    let remote = Command::new("git")
        .args(["rev-parse", "--short", "origin/main"])
        .current_dir(repo_path)
        .output()?;
    let new_commit = String::from_utf8_lossy(&remote.stdout).trim().to_string();

    // If same, we're up to date
    if current_commit == new_commit {
        return Ok(None);
    }

    // Count commits behind
    let count = Command::new("git")
        .args(["rev-list", "--count", "HEAD..origin/main"])
        .current_dir(repo_path)
        .output()?;
    let commits_behind: usize = String::from_utf8_lossy(&count.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    // Get latest commit message
    let msg = Command::new("git")
        .args(["log", "-1", "--format=%s", "origin/main"])
        .current_dir(repo_path)
        .output()?;
    let commit_message = String::from_utf8_lossy(&msg.stdout).trim().to_string();

    Ok(Some(UpdateInfo {
        current_commit,
        new_commit,
        commits_behind,
        commit_message,
    }))
}

/// Build update in background
/// 
/// Pulls latest changes, builds release binary, and copies to temp location.
/// Sends progress updates via the provided channel.
pub async fn build_update(
    repo_path: PathBuf,
    progress_tx: mpsc::UnboundedSender<UpdateStatus>,
) -> Result<PathBuf> {
    // Pull latest changes
    progress_tx.send(UpdateStatus::Building { 
        progress: "Pulling latest changes...".into() 
    })?;

    let pull = tokio::process::Command::new("git")
        .args(["pull", "origin", "main"])
        .current_dir(&repo_path)
        .output()
        .await?;

    if !pull.status.success() {
        let err = String::from_utf8_lossy(&pull.stderr);
        return Err(anyhow!("Git pull failed: {}", err));
    }

    // Build release binary
    progress_tx.send(UpdateStatus::Building { 
        progress: "Building release binary...".into() 
    })?;

    let build = tokio::process::Command::new("cargo")
        .args(["build", "--release", "-p", "krusty"])
        .current_dir(&repo_path)
        .output()
        .await?;

    if !build.status.success() {
        let err = String::from_utf8_lossy(&build.stderr);
        return Err(anyhow!("Cargo build failed: {}", err));
    }

    // Copy binary to temp location
    progress_tx.send(UpdateStatus::Building { 
        progress: "Preparing update...".into() 
    })?;

    let source = repo_path.join("target/release/krusty");
    let temp_dir = std::env::temp_dir();
    let dest = temp_dir.join("krusty-update");

    std::fs::copy(&source, &dest)?;
    
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;
    }

    progress_tx.send(UpdateStatus::Ready { new_binary: dest.clone() })?;

    Ok(dest)
}

/// Get the repo path from the current executable location
/// 
/// Assumes binary is either in target/release/krusty or ~/.local/bin/krusty
/// and the repo is the parent of target/.
pub fn detect_repo_path() -> Option<PathBuf> {
    // Try to find repo from executable path first
    if let Ok(exe) = std::env::current_exe() {
        // Check if we're in target/release
        if let Some(parent) = exe.parent() {
            if parent.ends_with("release") {
                if let Some(target) = parent.parent() {
                    if target.ends_with("target") {
                        if let Some(repo) = target.parent() {
                            if repo.join("Cargo.toml").exists() {
                                return Some(repo.to_path_buf());
                            }
                        }
                    }
                }
            }
        }
    }

    // Check if we're in the Krusty dev directory (for dev workflow)
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("Cargo.toml").exists() && cwd.join("crates/krusty-cli").exists() {
            return Some(cwd);
        }
    }

    // Check common dev location
    if let Some(home) = dirs::home_dir() {
        let common_path = home.join("Work/Krusty-Dev/krusty.dev");
        if common_path.join("Cargo.toml").exists() {
            return Some(common_path);
        }
    }

    None
}
