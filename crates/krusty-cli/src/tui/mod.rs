//! Terminal User Interface for Krusty

pub mod animation;
pub mod app;
pub mod blocks;
pub mod components;
pub mod graphics;
pub mod handlers;
pub mod input;
pub mod markdown;
pub mod polling;
pub mod popups;
pub mod render_model;
pub mod state;
pub mod streaming;
pub mod themes;
pub mod utils;

// Re-exports
pub use app::App;
