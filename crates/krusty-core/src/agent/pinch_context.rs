//! Pinch context for session transitions
//!
//! When context approaches limits, creates a structured context
//! to a new session with preserved context and user direction.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::summarizer::SummarizationResult;
use crate::storage::RankedFile;

/// Complete pinch context for injection into new session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinchContext {
    /// Source session UUID
    pub source_session_id: String,
    /// Source session title for reference
    pub source_session_title: String,
    /// High-level summary of work accomplished
    pub work_summary: String,
    /// Key architectural/design decisions made
    pub key_decisions: Vec<String>,
    /// Incomplete tasks or next steps
    pub pending_tasks: Vec<String>,
    /// Files ranked by importance
    pub ranked_files: Vec<RankedFileInfo>,
    /// User's hints about what to preserve (stage 1 input)
    pub preservation_hints: Option<String>,
    /// User's direction for next phase (stage 2 input)
    pub direction: Option<String>,
    /// When pinch was created
    pub created_at: DateTime<Utc>,
    /// CLAUDE.md / KRAB.md project context
    pub project_context: Option<String>,
    /// Key file contents (path, content) for immediate reference
    pub key_file_contents: Vec<(String, String)>,
    /// Active plan content (if any) - full markdown of the plan
    pub active_plan: Option<String>,
}

/// Serializable version of RankedFile for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedFileInfo {
    pub path: String,
    pub score: f64,
    pub reasons: Vec<String>,
}

impl From<RankedFile> for RankedFileInfo {
    fn from(rf: RankedFile) -> Self {
        Self {
            path: rf.path,
            score: rf.score,
            reasons: rf.reasons,
        }
    }
}

impl PinchContext {
    /// Create a new pinch context
    pub fn new(
        source_session_id: String,
        source_session_title: String,
        summary: SummarizationResult,
        ranked_files: Vec<RankedFile>,
        preservation_hints: Option<String>,
        direction: Option<String>,
        project_context: Option<String>,
        key_file_contents: Vec<(String, String)>,
        active_plan: Option<String>,
    ) -> Self {
        Self {
            source_session_id,
            source_session_title,
            work_summary: summary.work_summary,
            key_decisions: summary.key_decisions,
            pending_tasks: summary.pending_tasks,
            ranked_files: ranked_files.into_iter().map(Into::into).collect(),
            preservation_hints,
            direction,
            created_at: Utc::now(),
            project_context,
            key_file_contents,
            active_plan,
        }
    }

    /// Format as system message for new session
    ///
    /// Creates a structured markdown document that provides
    /// context for the continued conversation.
    pub fn to_system_message(&self) -> String {
        let mut msg = String::new();

        // Directive header - tell Claude to USE this context
        msg.push_str("# Pinch - CONTINUATION SESSION\n\n");
        msg.push_str("> **IMPORTANT**: This is a continuation of previous work. ");
        msg.push_str("The context below represents completed analysis and decisions. ");
        msg.push_str("Do NOT re-search or re-discover what is already documented here. ");
        msg.push_str("Start from this context and continue the work.\n\n");

        msg.push_str(&format!(
            "Continuing from session: **{}**\n\n",
            self.source_session_title
        ));

        // User direction (highest priority - put first)
        if let Some(direction) = &self.direction {
            msg.push_str("## Priority Direction\n\n");
            msg.push_str(&format!("**User requested focus**: {}\n\n", direction));
        }

        // Work summary
        msg.push_str("## Summary of Previous Work\n\n");
        msg.push_str(&self.work_summary);
        msg.push_str("\n\n");

        // Key decisions
        if !self.key_decisions.is_empty() {
            msg.push_str("## Key Decisions Made\n\n");
            for decision in &self.key_decisions {
                msg.push_str(&format!("- {}\n", decision));
            }
            msg.push('\n');
        }

        // Pending tasks - make these actionable
        if !self.pending_tasks.is_empty() {
            msg.push_str("## Pending/Incomplete Work (Continue From Here)\n\n");
            msg.push_str("These tasks were identified as next steps:\n\n");
            for (i, task) in self.pending_tasks.iter().enumerate() {
                msg.push_str(&format!("{}. {}\n", i + 1, task));
            }
            msg.push('\n');
        }

        // Key files list (top 10)
        if !self.ranked_files.is_empty() {
            msg.push_str("## Key Files (by importance)\n\n");
            for (i, file) in self.ranked_files.iter().take(10).enumerate() {
                let reasons = if file.reasons.is_empty() {
                    String::new()
                } else {
                    format!(" - {}", file.reasons.join(", "))
                };
                msg.push_str(&format!("{}. `{}`{}\n", i + 1, file.path, reasons));
            }
            msg.push('\n');
        }

        // Preservation hints (if any)
        if let Some(hints) = &self.preservation_hints {
            msg.push_str("## Preservation Notes\n\n");
            msg.push_str(&format!("User emphasized: {}\n\n", hints));
        }

        msg.push_str("---\n\n");

        // PROJECT CONTEXT (CLAUDE.md) - Critical for continuation!
        if let Some(ctx) = &self.project_context {
            msg.push_str("## Project Instructions (CLAUDE.md)\n\n");
            msg.push_str("Follow these project rules and guidelines:\n\n");
            // Truncate if extremely long (keep most important parts)
            if ctx.len() > 8000 {
                msg.push_str(&ctx[..8000]);
                msg.push_str("\n\n...[truncated for context limits]\n");
            } else {
                msg.push_str(ctx);
            }
            msg.push_str("\n\n---\n\n");
        }

        // KEY FILE CONTENTS - So Claude doesn't start blind
        if !self.key_file_contents.is_empty() {
            msg.push_str("## Key File Contents (Pre-loaded)\n\n");
            msg.push_str("These files are critical for continuing the work:\n\n");
            for (path, content) in self.key_file_contents.iter().take(5) {
                msg.push_str(&format!("### `{}`\n\n```\n", path));
                // Truncate very long files
                if content.len() > 4000 {
                    msg.push_str(&content[..4000]);
                    msg.push_str("\n...[truncated]\n");
                } else {
                    msg.push_str(content);
                }
                msg.push_str("\n```\n\n");
            }
            msg.push_str("---\n\n");
        }

        // ACTIVE PLAN - If user has a plan in progress
        if let Some(plan) = &self.active_plan {
            msg.push_str("## Active Plan\n\n");
            msg.push_str(
                "There is an active plan in progress. Continue from where you left off:\n\n",
            );
            // Truncate if very long
            if plan.len() > 6000 {
                msg.push_str(&plan[..6000]);
                msg.push_str("\n\n...[plan truncated]\n");
            } else {
                msg.push_str(plan);
            }
            msg.push_str("\n\n---\n\n");
        }

        // Action instruction
        msg.push_str("## How to Proceed\n\n");
        if self.direction.is_some() {
            msg.push_str("1. Focus on the **Priority Direction** above\n");
            msg.push_str("2. Reference the pre-loaded file contents above\n");
            msg.push_str("3. Build on the Key Decisions already made\n");
            msg.push_str("4. Read additional files as needed using the Key Files list\n");
        } else if !self.pending_tasks.is_empty() {
            msg.push_str("1. Start with the first **Pending Task** above\n");
            msg.push_str("2. Reference the pre-loaded file contents above\n");
            msg.push_str("3. Build on the Key Decisions already made\n");
            msg.push_str("4. Read additional files as needed using the Key Files list\n");
        } else {
            msg.push_str("Ask the user what they'd like to work on next.\n");
        }

        msg.push_str(&format!(
            "\n*Pinch created: {}*\n",
            self.created_at.format("%Y-%m-%d %H:%M UTC")
        ));

        msg
    }
}
