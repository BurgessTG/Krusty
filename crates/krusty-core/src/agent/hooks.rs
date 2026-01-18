//! Hook system for tool execution
//!
//! Allows intercepting tool calls before and after execution
//! for logging, validation, and safety.
//!
//! ## Built-in Hooks
//! - `SafetyHook` - Blocks dangerous bash commands (rm -rf, sudo, etc.)
//! - `LoggingHook` - Logs all tool executions with timing
//!
//! ## Custom Hooks
//! Implement `PreToolHook` or `PostToolHook` traits for custom behavior.

use crate::tools::registry::{ToolContext, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

/// Result of a hook execution
#[derive(Debug)]
pub enum HookResult {
    /// Continue with execution (no changes)
    Continue,
    /// Block execution with a reason
    Block { reason: String },
}

/// Hook called before tool execution
#[async_trait]
pub trait PreToolHook: Send + Sync {
    /// Called before a tool executes
    ///
    /// Returns:
    /// - `Continue` to proceed normally
    /// - `Modify(params)` to change the parameters
    /// - `Block { reason }` to prevent execution
    async fn before_execute(&self, name: &str, params: &Value, ctx: &ToolContext) -> HookResult;
}

/// Hook called after tool execution
#[async_trait]
pub trait PostToolHook: Send + Sync {
    /// Called after a tool executes
    ///
    /// Can inspect the result and duration but typically just logs.
    /// Returns `HookResult` for potential future use (result modification).
    async fn after_execute(
        &self,
        name: &str,
        params: &Value,
        result: &ToolResult,
        duration: Duration,
    ) -> HookResult;
}

// ============================================================================
// Built-in Hooks
// ============================================================================

/// Safety hook that blocks dangerous bash commands
///
/// Blocks commands containing:
/// - `rm -rf /` or similar destructive patterns
/// - `sudo` (requires explicit approval)
/// - `chmod 777` (overly permissive)
/// - `> /dev/sda` or similar disk writes
pub struct SafetyHook {
    /// Patterns to block (checked against command string)
    blocked_patterns: Vec<&'static str>,
}

impl Default for SafetyHook {
    fn default() -> Self {
        Self {
            blocked_patterns: vec![
                "rm -rf /",
                "rm -rf /*",
                "rm -rf ~",
                "sudo ",
                "chmod 777",
                "> /dev/sd",
                "dd if=",
                "mkfs.",
                ":(){:|:&};:", // Fork bomb
                "curl | sh",
                "curl | bash",
                "wget | sh",
                "wget | bash",
            ],
        }
    }
}

impl SafetyHook {
    pub fn new() -> Self {
        Self::default()
    }

    fn check_command(&self, command: &str) -> Option<&'static str> {
        let cmd_lower = command.to_lowercase();
        self.blocked_patterns
            .iter()
            .find(|pattern| cmd_lower.contains(&pattern.to_lowercase()))
            .copied()
    }
}

#[async_trait]
impl PreToolHook for SafetyHook {
    async fn before_execute(&self, name: &str, params: &Value, _ctx: &ToolContext) -> HookResult {
        // Only check bash/shell tools
        if name != "bash" && name != "shell" && name != "execute" {
            return HookResult::Continue;
        }

        // Extract command from params
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");

        if let Some(pattern) = self.check_command(command) {
            tracing::warn!(
                tool = name,
                command = command,
                blocked_pattern = pattern,
                "Safety hook blocked dangerous command"
            );
            return HookResult::Block {
                reason: format!("Blocked dangerous pattern: '{}'", pattern),
            };
        }

        HookResult::Continue
    }
}

/// Plan mode hook that blocks write tools in plan mode
///
/// When plan mode is active, blocks:
/// - Write, Edit, NotebookEdit (file modification tools)
/// - Bash commands that modify (rm, mv, mkdir, git commit, etc.)
///
/// Allows:
/// - Read, Glob, Grep, WebFetch, WebSearch
/// - Read-only bash commands (ls, cat, git status, git diff, etc.)
pub struct PlanModeHook {
    /// Write tools to block
    blocked_tools: Vec<&'static str>,
    /// Bash command prefixes to block
    blocked_bash_prefixes: Vec<&'static str>,
}

impl Default for PlanModeHook {
    fn default() -> Self {
        Self {
            blocked_tools: vec!["write", "edit", "notebook_edit", "build"],
            blocked_bash_prefixes: vec![
                "rm ",
                "rm\t",
                "rmdir",
                "mkdir",
                "mv ",
                "cp ",
                "touch ",
                "chmod ",
                "chown ",
                "ln ",
                "git add",
                "git commit",
                "git push",
                "git merge",
                "git rebase",
                "git reset",
                "git checkout -b",
                "git stash",
                "git cherry-pick",
                "git revert",
                "npm install",
                "npm uninstall",
                "yarn add",
                "yarn remove",
                "pip install",
                "pip uninstall",
                "cargo install",
                "make install",
                "cmake ",
                "ninja ",
                "echo >",
                "cat >",
                "tee ",
                "> ",
                ">> ",
            ],
        }
    }
}

impl PlanModeHook {
    pub fn new() -> Self {
        Self::default()
    }

    fn is_modifying_bash(&self, command: &str) -> bool {
        let cmd_lower = command.to_lowercase().trim_start().to_string();
        self.blocked_bash_prefixes
            .iter()
            .any(|prefix| cmd_lower.starts_with(&prefix.to_lowercase()))
    }
}

#[async_trait]
impl PreToolHook for PlanModeHook {
    async fn before_execute(&self, name: &str, params: &Value, ctx: &ToolContext) -> HookResult {
        // Only enforce in plan mode
        if !ctx.plan_mode {
            return HookResult::Continue;
        }

        // Block write tools
        if self.blocked_tools.contains(&name) {
            tracing::info!(tool = name, "Plan mode blocked write tool");
            return HookResult::Block {
                reason: format!(
                    "Tool '{}' is blocked in plan mode. Use Ctrl+B to exit plan mode first.",
                    name
                ),
            };
        }

        // Check bash commands
        if name == "bash" || name == "shell" || name == "execute" {
            let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");

            if self.is_modifying_bash(command) {
                tracing::info!(
                    tool = name,
                    command = command,
                    "Plan mode blocked modifying bash command"
                );
                return HookResult::Block {
                    reason: "Modifying bash commands are blocked in plan mode. Use Ctrl+B to exit plan mode first.".to_string(),
                };
            }
        }

        HookResult::Continue
    }
}

/// Logging hook that logs all tool executions
pub struct LoggingHook;

impl LoggingHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LoggingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PostToolHook for LoggingHook {
    async fn after_execute(
        &self,
        name: &str,
        _params: &Value,
        result: &ToolResult,
        duration: Duration,
    ) -> HookResult {
        tracing::info!(
            tool = name,
            duration_ms = duration.as_millis() as u64,
            is_error = result.is_error,
            output_len = result.output.len(),
            "Tool execution completed"
        );
        HookResult::Continue
    }
}
