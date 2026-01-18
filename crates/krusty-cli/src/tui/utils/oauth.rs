//! OAuth flow handling for the TUI
//!
//! Extracted from app.rs for better separation of concerns.

use crate::auth::{
    finish_oauth_flow, start_oauth_flow as auth_start_oauth_flow, TokenResponse,
};
use crate::paths;

use crate::tui::app::App;

impl App {
    /// Start the OAuth flow - opens browser and waits for user to paste callback
    pub fn start_oauth_flow(&mut self) {
        // Get auth URL and verifier
        let (auth_url, verifier) = auth_start_oauth_flow();

        // Store verifier for later
        self.oauth_verifier = Some(verifier);

        // Open browser
        if let Err(e) = webbrowser::open(&auth_url) {
            tracing::warn!("Failed to open browser: {}", e);
        }

        // Update popup to show the URL and wait for code input
        self.popups.auth.set_oauth_url(auth_url);
    }

    /// Exchange the OAuth callback URL/code for tokens
    pub fn exchange_oauth_code(&mut self, callback: String) {
        // Take the verifier
        let verifier = match self.oauth_verifier.take() {
            Some(v) => v,
            None => {
                self.popups
                    .auth
                    .set_oauth_error("No OAuth flow in progress".to_string());
                return;
            }
        };

        // Update UI
        self.popups.auth.set_oauth_exchanging();

        // Create channel for result
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.channels.oauth = Some(rx);

        // Exchange in background
        tokio::spawn(async move {
            let result = finish_oauth_flow(callback, verifier).await;
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
    }

    /// Save OAuth tokens to file
    pub async fn save_oauth_tokens(&self, token_response: &TokenResponse) -> anyhow::Result<()> {
        let token_path = paths::tokens_dir().join("anthropic_oauth.json");

        // Create directory if needed
        if let Some(parent) = token_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Calculate expiration
        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(token_response.expires_in as i64 - 300);

        let token_data = serde_json::json!({
            "access_token": token_response.access_token,
            "refresh_token": token_response.refresh_token,
            "expires_at": expires_at.to_rfc3339(),
            "token_type": token_response.token_type,
            "scope": token_response.scope,
        });

        tokio::fs::write(&token_path, serde_json::to_string_pretty(&token_data)?).await?;

        // Set restrictive permissions on Unix (read/write for owner only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(&token_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            tokio::fs::set_permissions(&token_path, permissions).await?;
        }

        tracing::info!("Saved OAuth tokens to {:?}", token_path);

        Ok(())
    }
}
