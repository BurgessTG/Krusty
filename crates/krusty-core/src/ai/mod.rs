//! AI provider layer
//!
//! Handles communication with AI providers (Anthropic Claude and compatible APIs)

pub mod anthropic;
pub mod glm;
pub mod models;
pub mod opencodezen;
pub mod openrouter;
pub mod providers;
pub mod sse;
pub mod stream_buffer;
pub mod streaming;
pub mod title;
pub mod transform;
pub mod types;

pub use title::{generate_pinch_title, generate_title};
