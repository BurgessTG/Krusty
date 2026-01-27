//! Dialogue types for Big Claw / Little Claw communication

use super::ClawRole;

/// A single turn in the dialogue
#[derive(Debug, Clone)]
pub struct DialogueTurn {
    pub speaker: Speaker,
    pub content: String,
}

/// Who is speaking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    BigClaw,
    LittleClaw,
}

impl Speaker {
    pub fn display_name(&self) -> &'static str {
        match self {
            Speaker::BigClaw => "Big Claw",
            Speaker::LittleClaw => "Little Claw",
        }
    }

    pub fn from_role(role: ClawRole) -> Self {
        match role {
            ClawRole::BigClaw => Speaker::BigClaw,
            ClawRole::LittleClaw => Speaker::LittleClaw,
        }
    }
}

/// Result of a dialogue exchange
#[derive(Debug)]
pub enum DialogueResult {
    /// Both agreed, proceed with action
    Consensus { dialogue: Vec<DialogueTurn> },

    /// Little Claw raised concerns that were addressed
    Refined { dialogue: Vec<DialogueTurn> },

    /// Little Claw found issues post-action, needs enhancement
    NeedsEnhancement {
        dialogue: Vec<DialogueTurn>,
        critique: String,
    },

    /// Couldn't agree, Big Claw proceeds with noted concern
    BigClawDecides {
        dialogue: Vec<DialogueTurn>,
        concern: String,
    },

    /// Dual-mind disabled or trivial action
    Skipped,
}

impl DialogueResult {
    /// Check if this result requires action refinement
    pub fn needs_enhancement(&self) -> bool {
        matches!(self, DialogueResult::NeedsEnhancement { .. })
    }

    /// Get the dialogue transcript
    pub fn dialogue(&self) -> &[DialogueTurn] {
        match self {
            DialogueResult::Consensus { dialogue }
            | DialogueResult::Refined { dialogue }
            | DialogueResult::NeedsEnhancement { dialogue, .. }
            | DialogueResult::BigClawDecides { dialogue, .. } => dialogue,
            DialogueResult::Skipped => &[],
        }
    }

    /// Format dialogue for display
    pub fn format_dialogue(&self) -> String {
        self.dialogue()
            .iter()
            .map(|turn| format!("[{}] {}", turn.speaker.display_name(), turn.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
