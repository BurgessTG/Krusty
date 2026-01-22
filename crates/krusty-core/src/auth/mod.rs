//! Authentication for Krusty
//!
//! This module provides OAuth authentication support for providers that support it,
//! as well as the types and utilities needed for authentication flows.
//!
//! API key storage is handled by the credentials module in storage/

pub mod browser_flow;
pub mod device_flow;
pub mod pkce;
pub mod providers;
pub mod storage;
pub mod types;

// Re-exports for convenience
pub use browser_flow::{open_browser, BrowserOAuthFlow, DEFAULT_CALLBACK_PORT};
pub use device_flow::{DeviceCodeFlow, DeviceCodeResponse};
pub use pkce::{PkceChallenge, PkceVerifier};
pub use providers::openai_oauth_config;
pub use storage::OAuthTokenStore;
pub use types::{AuthMethod, OAuthConfig, OAuthTokenData};
