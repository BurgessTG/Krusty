//! Cancellation support for agent tasks
//!
//! Allows interrupting running API calls and tool executions.

use tokio_util::sync::CancellationToken;

/// Wrapper around CancellationToken for agent task cancellation
#[derive(Clone)]
pub struct AgentCancellation {
    token: CancellationToken,
}

impl AgentCancellation {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Cancel all tasks using this token
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Get a child token for a subtask
    pub fn child_token(&self) -> CancellationToken {
        self.token.child_token()
    }

    /// Create a fresh token (for starting a new request)
    pub fn reset(&mut self) {
        self.token = CancellationToken::new();
    }
}

impl Default for AgentCancellation {
    fn default() -> Self {
        Self::new()
    }
}
