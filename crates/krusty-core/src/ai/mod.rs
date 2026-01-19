//! AI provider layer
//!
//! Handles communication with AI providers (Anthropic, OpenRouter, OpenCodeZen, etc.)
//! Supports multiple API formats: Anthropic, OpenAI, and Google.

// Modular architecture
pub mod client;
pub mod format;
pub mod retry;

// Provider-specific configuration
pub mod glm;
pub mod models;
pub mod opencodezen;
pub mod openrouter;

// Shared infrastructure
pub mod parsers;
pub mod providers;
pub mod reasoning;
pub mod sse;
pub mod stream_buffer;
pub mod streaming;
pub mod title;
pub mod transform;
pub mod types;

// Re-export main types from new module
pub use client::{AiClient, AiClientConfig, CallOptions, KRUSTY_SYSTEM_PROMPT};

// Backwards compatibility re-exports (deprecated)
#[allow(deprecated)]
pub use client::{AnthropicClient, AnthropicConfig};

pub use title::{generate_pinch_title, generate_title};
