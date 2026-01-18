//! Shared build context for builder swarm coordination
//!
//! LEAN design - only what's actually used:
//! - Conventions (coding style rules)
//! - File locks (prevent concurrent edits)
//! - Line diffs (UI feedback)
//! - Modified files tracking (summary)

use dashmap::DashMap;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Shared context for builder swarm coordination
pub struct SharedBuildContext {
    /// Coding conventions (all builders follow these)
    conventions: RwLock<Vec<String>>,

    /// File locks: path -> agent_id holding the lock
    file_locks: DashMap<PathBuf, String>,

    /// Files modified during this build: path -> agent_id
    modified_files: DashMap<PathBuf, String>,

    /// Line diff tracking for UI
    lines_added: AtomicUsize,
    lines_removed: AtomicUsize,

    /// Stats for debugging
    locks_acquired: AtomicUsize,
    lock_contentions: AtomicUsize,
}

impl SharedBuildContext {
    pub fn new() -> Self {
        Self {
            conventions: RwLock::new(Vec::new()),
            file_locks: DashMap::new(),
            modified_files: DashMap::new(),
            lines_added: AtomicUsize::new(0),
            lines_removed: AtomicUsize::new(0),
            locks_acquired: AtomicUsize::new(0),
            lock_contentions: AtomicUsize::new(0),
        }
    }

    // =========================================================================
    // Conventions
    // =========================================================================

    /// Set conventions at start of build
    pub fn set_conventions(&self, conventions: Vec<String>) {
        *self.conventions.write() = conventions;
    }

    /// Get all conventions
    pub fn get_conventions(&self) -> Vec<String> {
        self.conventions.read().clone()
    }

    // =========================================================================
    // File Locks
    // =========================================================================

    /// Try to acquire a lock. Returns Ok(()) or Err(holder_id)
    pub fn acquire_lock(&self, path: PathBuf, agent_id: String, _reason: String) -> Result<(), String> {
        if let Some(holder) = self.file_locks.get(&path) {
            if *holder != agent_id {
                self.lock_contentions.fetch_add(1, Ordering::Relaxed);
                return Err(holder.clone());
            }
            return Ok(()); // Already held by this agent
        }

        self.file_locks.insert(path, agent_id);
        self.locks_acquired.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Release a file lock
    pub fn release_lock(&self, path: &PathBuf, agent_id: &str) {
        if let Some(holder) = self.file_locks.get(path) {
            if *holder == agent_id {
                drop(holder);
                self.file_locks.remove(path);
            }
        }
    }

    /// Release all locks held by an agent (cleanup)
    pub fn release_all_locks(&self, agent_id: &str) {
        let to_release: Vec<PathBuf> = self
            .file_locks
            .iter()
            .filter(|r| *r.value() == agent_id)
            .map(|r| r.key().clone())
            .collect();

        for path in to_release {
            self.file_locks.remove(&path);
        }
    }

    // =========================================================================
    // Modified Files
    // =========================================================================

    /// Record that a file was modified
    pub fn record_modification(&self, path: PathBuf, agent_id: String) {
        self.modified_files.insert(path, agent_id);
    }

    // =========================================================================
    // Line Diffs
    // =========================================================================

    /// Record line changes
    pub fn record_line_changes(&self, added: usize, removed: usize) {
        self.lines_added.fetch_add(added, Ordering::Relaxed);
        self.lines_removed.fetch_add(removed, Ordering::Relaxed);
    }

    /// Get current line diff totals
    pub fn get_line_diff(&self) -> (usize, usize) {
        (
            self.lines_added.load(Ordering::Relaxed),
            self.lines_removed.load(Ordering::Relaxed),
        )
    }

    // =========================================================================
    // Context Injection (for builder prompts)
    // =========================================================================

    /// Generate context to inject into builder prompts
    pub fn generate_context_injection(&self) -> String {
        let mut lines = Vec::new();

        // Conventions
        let conventions = self.get_conventions();
        if !conventions.is_empty() {
            lines.push("[CONVENTIONS]".to_string());
            for conv in conventions {
                lines.push(format!("- {}", conv));
            }
            lines.push(String::new());
        }

        // Current locks (so builders know what's being worked on)
        let locks: Vec<_> = self
            .file_locks
            .iter()
            .map(|r| (r.key().display().to_string(), r.value().clone()))
            .collect();
        if !locks.is_empty() {
            lines.push("[FILES IN PROGRESS]".to_string());
            for (path, agent) in locks {
                lines.push(format!("- {} (by {})", path, agent));
            }
            lines.push(String::new());
        }

        lines.join("\n")
    }

    // =========================================================================
    // Stats
    // =========================================================================

    pub fn stats(&self) -> BuildContextStats {
        BuildContextStats {
            files_modified: self.modified_files.len(),
            lines_added: self.lines_added.load(Ordering::Relaxed),
            lines_removed: self.lines_removed.load(Ordering::Relaxed),
            lock_contentions: self.lock_contentions.load(Ordering::Relaxed),
        }
    }
}

impl Default for SharedBuildContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Stats for logging/display
#[derive(Debug, Clone)]
pub struct BuildContextStats {
    pub files_modified: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub lock_contentions: usize,
}

impl std::fmt::Display for BuildContextStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "+{} -{} lines, {} files, {} contentions",
            self.lines_added, self.lines_removed, self.files_modified, self.lock_contentions
        )
    }
}
