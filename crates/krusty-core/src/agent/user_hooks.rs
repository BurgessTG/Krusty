//! User-configurable hooks system
//!
//! Allows users to define custom hooks that execute shell commands
//! before/after tool execution. Hooks can block, warn, or silently proceed
//! based on exit codes.
//!
//! ## Exit Code Protocol
//! - 0: Continue (stdout/stderr not shown)
//! - 2: Block tool execution, show stderr to model
//! - Other: Warn user with stderr, but continue

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Type of user hook
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserHookType {
    /// Runs before tool execution, can block
    PreToolUse,
    /// Runs after tool execution
    PostToolUse,
    /// Fires on notification events (non-blocking)
    Notification,
    /// Fires when user submits a prompt
    UserPromptSubmit,
}

impl UserHookType {
    /// All hook types for UI display
    pub fn all() -> &'static [UserHookType] {
        &[
            UserHookType::PreToolUse,
            UserHookType::PostToolUse,
            UserHookType::Notification,
            UserHookType::UserPromptSubmit,
        ]
    }

    /// Human-readable name
    pub fn display_name(&self) -> &'static str {
        match self {
            UserHookType::PreToolUse => "PreToolUse",
            UserHookType::PostToolUse => "PostToolUse",
            UserHookType::Notification => "Notification",
            UserHookType::UserPromptSubmit => "UserPromptSubmit",
        }
    }

    /// Description for UI
    pub fn description(&self) -> &'static str {
        match self {
            UserHookType::PreToolUse => "Before tool execution",
            UserHookType::PostToolUse => "After tool execution",
            UserHookType::Notification => "When notifications are sent",
            UserHookType::UserPromptSubmit => "When the user submits a prompt",
        }
    }

    /// Parse from string representation
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "PreToolUse" => Some(UserHookType::PreToolUse),
            "PostToolUse" => Some(UserHookType::PostToolUse),
            "Notification" => Some(UserHookType::Notification),
            "UserPromptSubmit" => Some(UserHookType::UserPromptSubmit),
            _ => None,
        }
    }
}

impl std::fmt::Display for UserHookType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// A user-defined hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserHook {
    /// Unique identifier
    pub id: String,
    /// Type of hook
    pub hook_type: UserHookType,
    /// Regex pattern to match tool names
    pub tool_pattern: String,
    /// Shell command to execute
    pub command: String,
    /// Whether this hook is enabled
    pub enabled: bool,
    /// When the hook was created
    pub created_at: String,
    /// Compiled regex (not serialized)
    #[serde(skip)]
    compiled_pattern: Option<Regex>,
}

impl UserHook {
    /// Create a new user hook
    pub fn new(hook_type: UserHookType, tool_pattern: String, command: String) -> Self {
        let compiled = Regex::new(&tool_pattern).ok();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            hook_type,
            tool_pattern,
            command,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
            compiled_pattern: compiled,
        }
    }

    /// Check if this hook matches a tool name
    pub fn matches(&mut self, tool_name: &str) -> bool {
        if !self.enabled {
            return false;
        }

        // Lazy compile the regex if needed
        if self.compiled_pattern.is_none() {
            self.compiled_pattern = Regex::new(&self.tool_pattern).ok();
        }

        self.compiled_pattern
            .as_ref()
            .map(|re| re.is_match(tool_name))
            .unwrap_or(false)
    }

    /// Compile the pattern (call after loading from DB)
    pub fn compile_pattern(&mut self) {
        self.compiled_pattern = Regex::new(&self.tool_pattern).ok();
    }

    /// Check if the pattern is valid regex
    pub fn is_pattern_valid(&self) -> bool {
        Regex::new(&self.tool_pattern).is_ok()
    }
}

/// Result of executing a user hook
#[derive(Debug)]
pub enum UserHookResult {
    /// Continue with tool execution
    Continue,
    /// Block tool execution with reason (exit code 2)
    Block { reason: String },
    /// Warning shown to user, but continue (other non-zero exit)
    Warn { message: String },
}

/// Manager for user hooks - handles CRUD and persistence
pub struct UserHookManager {
    hooks: Vec<UserHook>,
}

impl Default for UserHookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UserHookManager {
    /// Create a new empty manager
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Load hooks from database (legacy - no user filtering)
    pub fn load(&mut self, db: &crate::storage::Database) -> Result<()> {
        self.load_for_user(db, None)
    }

    /// Load hooks for a specific user (multi-tenant) or all hooks (single-tenant)
    pub fn load_for_user(
        &mut self,
        db: &crate::storage::Database,
        user_id: Option<&str>,
    ) -> Result<()> {
        use rusqlite::params;

        let conn = db.conn();
        let hooks: Vec<UserHook> = if let Some(uid) = user_id {
            // Multi-tenant: filter by user_id (NULL matches for backwards compat)
            let mut stmt = conn.prepare(
                "SELECT id, hook_type, tool_pattern, command, enabled, created_at
                 FROM user_hooks WHERE user_id = ?1 OR user_id IS NULL ORDER BY created_at",
            )?;
            let rows = stmt.query_map(params![uid], |row| {
                Ok(UserHook {
                    id: row.get(0)?,
                    hook_type: UserHookType::parse(&row.get::<_, String>(1)?)
                        .unwrap_or(UserHookType::PreToolUse),
                    tool_pattern: row.get(2)?,
                    command: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? != 0,
                    created_at: row.get(5)?,
                    compiled_pattern: None,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            // Single-tenant: load all hooks
            let mut stmt = conn.prepare(
                "SELECT id, hook_type, tool_pattern, command, enabled, created_at
                 FROM user_hooks ORDER BY created_at",
            )?;
            let rows = stmt.query_map(params![], |row| {
                Ok(UserHook {
                    id: row.get(0)?,
                    hook_type: UserHookType::parse(&row.get::<_, String>(1)?)
                        .unwrap_or(UserHookType::PreToolUse),
                    tool_pattern: row.get(2)?,
                    command: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? != 0,
                    created_at: row.get(5)?,
                    compiled_pattern: None,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        self.hooks.clear();
        for mut hook in hooks {
            hook.compile_pattern();
            self.hooks.push(hook);
        }

        Ok(())
    }

    /// Save a new hook to database (legacy - no user_id)
    pub fn save(&mut self, db: &crate::storage::Database, hook: UserHook) -> Result<()> {
        self.save_for_user(db, hook, None)
    }

    /// Save a new hook for a specific user
    pub fn save_for_user(
        &mut self,
        db: &crate::storage::Database,
        hook: UserHook,
        user_id: Option<&str>,
    ) -> Result<()> {
        use rusqlite::params;

        db.conn().execute(
            "INSERT INTO user_hooks (id, hook_type, tool_pattern, command, enabled, created_at, user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                hook.id,
                hook.hook_type.display_name(),
                hook.tool_pattern,
                hook.command,
                if hook.enabled { 1 } else { 0 },
                hook.created_at,
                user_id,
            ],
        )?;

        let mut h = hook;
        h.compile_pattern();
        self.hooks.push(h);
        Ok(())
    }

    /// Delete a hook by ID (validates ownership in multi-tenant mode)
    pub fn delete(&mut self, db: &crate::storage::Database, id: &str) -> Result<()> {
        self.delete_for_user(db, id, None)
    }

    /// Delete a hook for a specific user
    pub fn delete_for_user(
        &mut self,
        db: &crate::storage::Database,
        id: &str,
        user_id: Option<&str>,
    ) -> Result<()> {
        use rusqlite::params;

        if let Some(uid) = user_id {
            // Multi-tenant: only delete if owned by user (or has no owner)
            db.conn().execute(
                "DELETE FROM user_hooks WHERE id = ?1 AND (user_id = ?2 OR user_id IS NULL)",
                params![id, uid],
            )?;
        } else {
            db.conn()
                .execute("DELETE FROM user_hooks WHERE id = ?1", params![id])?;
        }
        self.hooks.retain(|h| h.id != id);
        Ok(())
    }

    /// Toggle a hook's enabled state (validates ownership in multi-tenant mode)
    pub fn toggle(&mut self, db: &crate::storage::Database, id: &str) -> Result<bool> {
        self.toggle_for_user(db, id, None)
    }

    /// Toggle a hook for a specific user
    pub fn toggle_for_user(
        &mut self,
        db: &crate::storage::Database,
        id: &str,
        user_id: Option<&str>,
    ) -> Result<bool> {
        use rusqlite::params;

        let hook = self.hooks.iter_mut().find(|h| h.id == id);
        if let Some(h) = hook {
            h.enabled = !h.enabled;
            if let Some(uid) = user_id {
                // Multi-tenant: only update if owned by user (or has no owner)
                db.conn().execute(
                    "UPDATE user_hooks SET enabled = ?1 WHERE id = ?2 AND (user_id = ?3 OR user_id IS NULL)",
                    params![if h.enabled { 1 } else { 0 }, id, uid],
                )?;
            } else {
                db.conn().execute(
                    "UPDATE user_hooks SET enabled = ?1 WHERE id = ?2",
                    params![if h.enabled { 1 } else { 0 }, id],
                )?;
            }
            return Ok(h.enabled);
        }
        Ok(false)
    }

    /// Get all hooks
    pub fn hooks(&self) -> &[UserHook] {
        &self.hooks
    }

    /// Get hooks by type
    pub fn hooks_by_type(&self, hook_type: UserHookType) -> Vec<&UserHook> {
        self.hooks
            .iter()
            .filter(|h| h.hook_type == hook_type)
            .collect()
    }

    /// Get enabled hooks that match a tool name
    pub fn matching_hooks(&mut self, hook_type: UserHookType, tool_name: &str) -> Vec<&UserHook> {
        // First pass: compile patterns and check matches, collect IDs
        let matching_ids: Vec<String> = self
            .hooks
            .iter_mut()
            .filter_map(|h| {
                if h.hook_type == hook_type && h.matches(tool_name) {
                    Some(h.id.clone())
                } else {
                    None
                }
            })
            .collect();

        // Second pass: return references
        self.hooks
            .iter()
            .filter(|h| matching_ids.contains(&h.id))
            .collect()
    }
}

/// Executor for user hooks - runs shell commands and interprets results
pub struct UserHookExecutor;

impl UserHookExecutor {
    /// Execute a hook command with JSON input
    ///
    /// The command receives JSON on stdin with tool call details.
    /// Exit codes:
    /// - 0: Continue (stdout/stderr not shown)
    /// - 2: Block tool, show stderr to model
    /// - Other: Warn user with stderr, continue
    pub async fn execute(hook: &UserHook, tool_name: &str, params: &serde_json::Value) -> UserHookResult {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;
        use tokio::process::Command;

        // Build JSON input for the hook
        let input = serde_json::json!({
            "tool_name": tool_name,
            "tool_input": params,
            "hook_id": hook.id,
            "hook_type": hook.hook_type.display_name(),
        });

        let input_str = match serde_json::to_string(&input) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(hook_id = %hook.id, "Failed to serialize hook input: {}", e);
                return UserHookResult::Continue;
            }
        };

        // Spawn shell process
        let mut child = match Command::new("sh")
            .arg("-c")
            .arg(&hook.command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(hook_id = %hook.id, command = %hook.command, "Failed to spawn hook: {}", e);
                return UserHookResult::Warn {
                    message: format!("Hook failed to spawn: {}", e),
                };
            }
        };

        // Write JSON input to stdin
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(input_str.as_bytes()).await {
                tracing::warn!(hook_id = %hook.id, "Failed to write to hook stdin: {}", e);
            }
            // Drop stdin to close it
        }

        // Wait for completion with timeout
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            child.wait_with_output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                tracing::warn!(hook_id = %hook.id, "Hook execution failed: {}", e);
                return UserHookResult::Warn {
                    message: format!("Hook execution failed: {}", e),
                };
            }
            Err(_) => {
                tracing::warn!(hook_id = %hook.id, "Hook timed out after 30s");
                return UserHookResult::Warn {
                    message: "Hook timed out after 30 seconds".to_string(),
                };
            }
        };

        let exit_code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        tracing::debug!(
            hook_id = %hook.id,
            exit_code,
            stderr_len = stderr.len(),
            "Hook execution complete"
        );

        match exit_code {
            0 => UserHookResult::Continue,
            2 => {
                // Block with stderr as reason
                let reason = if stderr.is_empty() {
                    "Hook blocked execution".to_string()
                } else {
                    stderr.trim().to_string()
                };
                UserHookResult::Block { reason }
            }
            _ => {
                // Warn but continue
                let message = if stderr.is_empty() {
                    format!("Hook exited with code {}", exit_code)
                } else {
                    stderr.trim().to_string()
                };
                UserHookResult::Warn { message }
            }
        }
    }

    /// Execute all matching hooks for a tool
    ///
    /// Returns Block if any hook blocks, otherwise Continue.
    /// Warnings are logged but don't stop execution.
    pub async fn execute_matching(
        manager: &mut UserHookManager,
        hook_type: UserHookType,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> UserHookResult {
        let hooks: Vec<UserHook> = manager
            .matching_hooks(hook_type, tool_name)
            .iter()
            .map(|h| (*h).clone())
            .collect();

        for hook in hooks {
            let result = Self::execute(&hook, tool_name, params).await;
            match result {
                UserHookResult::Block { reason } => {
                    tracing::info!(
                        hook_id = %hook.id,
                        tool = tool_name,
                        "User hook blocked execution: {}",
                        reason
                    );
                    return UserHookResult::Block { reason };
                }
                UserHookResult::Warn { message } => {
                    tracing::warn!(
                        hook_id = %hook.id,
                        tool = tool_name,
                        "User hook warning: {}",
                        message
                    );
                    // Continue checking other hooks
                }
                UserHookResult::Continue => {}
            }
        }

        UserHookResult::Continue
    }
}

// ============================================================================
// PreToolHook and PostToolHook trait implementations
// ============================================================================

use crate::agent::hooks::{HookResult, PostToolHook, PreToolHook};
use crate::tools::registry::{ToolContext, ToolResult};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Wrapper that implements PreToolHook for user-defined hooks
pub struct UserPreToolHook {
    manager: Arc<RwLock<UserHookManager>>,
}

impl UserPreToolHook {
    pub fn new(manager: Arc<RwLock<UserHookManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl PreToolHook for UserPreToolHook {
    async fn before_execute(
        &self,
        name: &str,
        params: &serde_json::Value,
        _ctx: &ToolContext,
    ) -> HookResult {
        let mut manager = self.manager.write().await;
        let result =
            UserHookExecutor::execute_matching(&mut manager, UserHookType::PreToolUse, name, params)
                .await;

        match result {
            UserHookResult::Block { reason } => HookResult::Block { reason },
            UserHookResult::Warn { message } => {
                // Log warning but continue
                tracing::warn!(tool = name, "User pre-hook warning: {}", message);
                HookResult::Continue
            }
            UserHookResult::Continue => HookResult::Continue,
        }
    }
}

/// Wrapper that implements PostToolHook for user-defined hooks
pub struct UserPostToolHook {
    manager: Arc<RwLock<UserHookManager>>,
}

impl UserPostToolHook {
    pub fn new(manager: Arc<RwLock<UserHookManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl PostToolHook for UserPostToolHook {
    async fn after_execute(
        &self,
        name: &str,
        params: &serde_json::Value,
        _result: &ToolResult,
        _duration: Duration,
    ) -> HookResult {
        let mut manager = self.manager.write().await;
        // Post hooks don't block, they just run
        let _ = UserHookExecutor::execute_matching(
            &mut manager,
            UserHookType::PostToolUse,
            name,
            params,
        )
        .await;

        HookResult::Continue
    }
}
