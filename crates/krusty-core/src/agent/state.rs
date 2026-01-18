//! Agent state tracking
//!
//! Tracks turn count and timing for safety limits.

use std::time::{Duration, Instant};

/// Runtime state of the agent
#[derive(Debug, Default)]
pub struct AgentState {
    /// Current turn number (increments each time we send to AI)
    pub current_turn: usize,
    /// When the current turn started
    pub turn_start: Option<Instant>,
    /// Whether the agent was interrupted
    pub is_interrupted: bool,
}

impl AgentState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new turn
    pub fn start_turn(&mut self) {
        self.current_turn += 1;
        self.turn_start = Some(Instant::now());
        self.is_interrupted = false;
    }

    /// Get duration of current turn
    pub fn turn_duration(&self) -> Option<Duration> {
        self.turn_start.map(|start| start.elapsed())
    }

    /// Mark as interrupted
    pub fn interrupt(&mut self) {
        self.is_interrupted = true;
        self.turn_start = None;
    }
}

/// Configuration for agent behavior
#[derive(Debug, Clone, Default)]
pub struct AgentConfig {
    /// Maximum turns before stopping (None = unlimited)
    pub max_turns: Option<usize>,
}

impl AgentConfig {
    /// Check if we've exceeded max turns
    pub fn exceeded_max_turns(&self, current_turn: usize) -> bool {
        self.max_turns.is_some_and(|max| current_turn >= max)
    }
}
