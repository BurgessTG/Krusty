//! Dual-Mind Agent System (Big Claw / Little Claw)
//!
//! A quality-focused architecture where two independent agents collaborate:
//! - **Big Claw**: The executor - does the work, has all tools
//! - **Little Claw**: The analyst - questions everything, ensures quality
//!
//! They share the same base philosophy (KRUSTY_SYSTEM_PROMPT) but have
//! different roles. Their dialogue appears in thinking blocks, invisible
//! to the user except for the resulting quality improvements.
//!
//! ## Key Principles
//! - Independent contexts (different perspectives)
//! - Same model (intelligence matters for quality)
//! - Little Claw can research but not edit
//! - Disagreements resolved through discussion + research
//! - Big Claw makes final call if deadlocked

mod dialogue;
mod little_claw;
mod observation;
mod roles;

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

use crate::ai::client::AiClient;
use crate::tools::ToolRegistry;

pub use dialogue::{DialogueResult, DialogueTurn, Speaker};
pub use little_claw::{extract_insight_patterns, is_trivial_response, LittleClaw};
pub use observation::{Observation, ObservedAction};
pub use roles::ClawRole;

/// Configuration for the dual-mind system
#[derive(Debug, Clone)]
pub struct DualMindConfig {
    /// Enable/disable the dual-mind system
    pub enabled: bool,
    /// Review every action, or only significant ones
    pub review_all: bool,
    /// Maximum discussion depth before Big Claw decides
    pub max_discussion_depth: usize,
}

impl Default for DualMindConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            review_all: true,
            max_discussion_depth: 5,
        }
    }
}

/// The dual-mind orchestrator
///
/// Manages two independent agent contexts that work together.
/// Big Claw executes, Little Claw questions and validates.
pub struct DualMind {
    /// Little Claw instance
    little_claw: LittleClaw,

    /// Configuration
    config: DualMindConfig,

    /// Accumulated dialogue for current action
    current_dialogue: Vec<DialogueTurn>,
}

impl DualMind {
    /// Create a new dual-mind system
    pub fn new(client: Arc<AiClient>) -> Self {
        Self {
            little_claw: LittleClaw::new(client),
            config: DualMindConfig::default(),
            current_dialogue: Vec::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(client: Arc<AiClient>, config: DualMindConfig) -> Self {
        let mut dual_mind = Self::new(client);
        dual_mind.config = config;
        dual_mind
    }

    /// Create with tools for Little Claw research
    pub fn with_tools(
        client: Arc<AiClient>,
        config: DualMindConfig,
        tools: Arc<ToolRegistry>,
        working_dir: PathBuf,
    ) -> Self {
        let little_claw = LittleClaw::new(client)
            .with_tools(tools)
            .with_working_dir(working_dir);

        Self {
            little_claw,
            config,
            current_dialogue: Vec::new(),
        }
    }

    /// Disable dual-mind (Big Claw operates alone)
    pub fn disable(&mut self) {
        self.config.enabled = false;
    }

    /// Enable dual-mind
    pub fn enable(&mut self) {
        self.config.enabled = true;
    }

    /// Check if dual-mind is active
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Sync Big Claw's action to Little Claw's context
    pub async fn sync_observation(&self, observation: Observation) {
        if !self.config.enabled {
            return;
        }
        self.little_claw.observe(observation).await;
    }

    /// Get the accumulated dialogue and clear it
    pub fn take_dialogue(&mut self) -> Vec<DialogueTurn> {
        std::mem::take(&mut self.current_dialogue)
    }

    /// Format dialogue for thinking block display
    pub fn format_dialogue(&self) -> String {
        let mut output = String::new();
        for turn in &self.current_dialogue {
            output.push_str(&format!(
                "[{}] {}\n\n",
                turn.speaker.display_name(),
                turn.content
            ));
        }
        output
    }

    /// Pre-action review: Little Claw questions Big Claw's intent
    ///
    /// Called before Big Claw executes a significant action.
    /// Returns the dialogue result.
    pub async fn pre_review(&mut self, intent: &str) -> DialogueResult {
        if !self.config.enabled {
            Self::log_to_file("[PRE-REVIEW] SKIPPED: dual-mind disabled");
            return DialogueResult::Skipped;
        }

        // Check if this is trivial (skip review)
        if self.is_trivial_action(intent) {
            debug!("Skipping pre-review for trivial action");
            Self::log_to_file(&format!(
                "[PRE-REVIEW] SKIPPED (trivial): {}",
                &intent.chars().take(100).collect::<String>()
            ));
            return DialogueResult::Skipped;
        }

        // Prevent infinite discussion loops
        if self.at_max_depth() {
            debug!(
                depth = self.dialogue_depth(),
                max = self.config.max_discussion_depth * 2,
                "Hit max discussion depth, Big Claw proceeds"
            );
            Self::log_to_file(&format!(
                "[PRE-REVIEW] SKIPPED (max depth {}): {}",
                self.dialogue_depth(),
                &intent.chars().take(100).collect::<String>()
            ));
            return DialogueResult::Skipped;
        }

        Self::log_to_file(&format!(
            "[PRE-REVIEW] PROCEEDING with review for: {}",
            &intent.chars().take(200).collect::<String>()
        ));
        info!("Little Claw reviewing intent before action");

        // Add Big Claw's intent to dialogue
        self.current_dialogue.push(DialogueTurn {
            speaker: Speaker::BigClaw,
            content: intent.to_string(),
        });

        // Get Little Claw's review
        let result = self.little_claw.review_intent(intent).await;

        // Add Little Claw's response to dialogue
        for turn in result.dialogue() {
            self.current_dialogue.push(turn.clone());
        }

        result
    }

    /// Post-action review: Little Claw validates the output
    ///
    /// Called after Big Claw produces output.
    /// Returns Enhancement if quality issues found.
    pub async fn post_review(&mut self, output: &str) -> DialogueResult {
        if !self.config.enabled {
            Self::log_to_file("[POST-REVIEW] SKIPPED: dual-mind disabled");
            return DialogueResult::Skipped;
        }

        // Prevent infinite discussion loops
        if self.at_max_depth() {
            debug!(
                depth = self.dialogue_depth(),
                max = self.config.max_discussion_depth * 2,
                "Hit max discussion depth, skipping post-review"
            );
            Self::log_to_file(&format!(
                "[POST-REVIEW] SKIPPED (max depth {})",
                self.dialogue_depth()
            ));
            return DialogueResult::Skipped;
        }

        Self::log_to_file(&format!(
            "[POST-REVIEW] PROCEEDING with review ({} chars output)",
            output.len()
        ));
        info!("Little Claw reviewing output after action");

        let result = self.little_claw.review_output(output).await;

        // Add Little Claw's review to dialogue
        for turn in result.dialogue() {
            self.current_dialogue.push(turn.clone());
        }

        result
    }

    /// Check if an action is trivial (not worth reviewing)
    fn is_trivial_action(&self, intent: &str) -> bool {
        if self.config.review_all {
            return false;
        }

        let lower = intent.to_lowercase();

        // Trivial patterns
        let trivial_patterns = [
            "fix typo",
            "typo",
            "whitespace",
            "formatting",
            "add comment",
            "remove comment",
        ];

        trivial_patterns.iter().any(|p| lower.contains(p))
    }

    /// DEBUG: Log to /tmp/dual_mind_dialogue.log for verification
    fn log_to_file(msg: &str) {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/dual_mind_dialogue.log")
        {
            use std::io::Write;
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
        }
    }

    /// Get Little Claw reference for direct interaction
    pub fn little_claw(&self) -> &LittleClaw {
        &self.little_claw
    }

    /// Get mutable Little Claw reference
    pub fn little_claw_mut(&mut self) -> &mut LittleClaw {
        &mut self.little_claw
    }

    /// Clear all state for new session
    pub async fn clear(&mut self) {
        self.little_claw.clear().await;
        self.current_dialogue.clear();
    }

    /// Get current dialogue depth
    pub fn dialogue_depth(&self) -> usize {
        self.current_dialogue.len()
    }

    /// Check if we've hit max discussion depth
    pub fn at_max_depth(&self) -> bool {
        self.dialogue_depth() >= self.config.max_discussion_depth * 2
    }
}

/// Builder for DualMind
pub struct DualMindBuilder {
    client: Arc<AiClient>,
    config: DualMindConfig,
}

impl DualMindBuilder {
    pub fn new(client: Arc<AiClient>) -> Self {
        Self {
            client,
            config: DualMindConfig::default(),
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    pub fn review_all(mut self, review_all: bool) -> Self {
        self.config.review_all = review_all;
        self
    }

    pub fn max_discussion_depth(mut self, depth: usize) -> Self {
        self.config.max_discussion_depth = depth;
        self
    }

    pub fn build(self) -> DualMind {
        DualMind::with_config(self.client, self.config)
    }
}
