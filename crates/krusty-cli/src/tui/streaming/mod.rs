//! Streaming State Machine
//!
//! Replaces fragile flag-based streaming state with a proper enum state machine.
//! All streaming state is centralized here for clarity and correctness.

pub mod state;

pub use state::{StreamEvent, StreamingManager};
