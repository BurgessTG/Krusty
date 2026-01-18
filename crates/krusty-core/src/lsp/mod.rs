//! LSP (Language Server Protocol) client infrastructure
//!
//! Provides JSON-RPC communication with language servers for:
//! - Diagnostics (errors, warnings)
//! - Go to definition
//! - Find references
//! - Hover information

pub mod builtin;
pub mod client;
pub mod diagnostics;
pub mod downloader;
pub mod manager;
pub mod transport;

pub use downloader::LspDownloader;
pub use manager::LspManager;
