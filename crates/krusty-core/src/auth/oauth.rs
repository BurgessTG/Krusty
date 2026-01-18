//! OAuth PKCE flow for Anthropic Claude
//!
//! Implements the OAuth 2.0 authorization code flow with PKCE
//! for authenticating with Anthropic's Claude API.
//!
//! NOTE: Anthropic OAuth does NOT support localhost redirect URIs.
//! Users must manually copy-paste the callback URL back into the app.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{thread_rng, RngCore};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

// OAuth configuration - these are the official Claude Code client credentials
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTH_ENDPOINT: &str = "https://claude.ai/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://console.anthropic.com/v1/oauth/token";
// CRITICAL: Must use console.anthropic.com redirect - localhost NOT supported!
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
    pub scope: String,
}

pub struct OAuthClient {
    http_client: Client,
}

#[derive(Debug)]
pub struct PkceVerifier(pub String);

impl OAuthClient {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .user_agent("Krusty/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();

        Self { http_client }
    }

    /// Get the authorization URL and PKCE verifier
    ///
    /// The user must visit this URL in their browser, authorize the app,
    /// then copy the resulting callback URL (which will show an error page
    /// but contain the authorization code) back into the app.
    pub fn get_auth_url(&self) -> (String, PkceVerifier) {
        // Generate PKCE verifier (43-128 chars)
        let mut rng = thread_rng();
        let mut verifier_bytes = vec![0u8; 32];
        rng.fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(&verifier_bytes);

        // Generate challenge (SHA256 of verifier)
        let mut hasher = Sha256::new();
        hasher.update(&verifier);
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

        // Build auth URL
        let mut auth_url = Url::parse(AUTH_ENDPOINT).unwrap();
        auth_url
            .query_pairs_mut()
            .append_pair("code", "true")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", REDIRECT_URI)
            .append_pair("scope", "user:profile user:inference user:sessions:claude_code")
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &verifier);

        (auth_url.to_string(), PkceVerifier(verifier))
    }

    /// Exchange authorization code for tokens
    ///
    /// The code can be provided as:
    /// - Just the code string
    /// - A full callback URL containing `code=...`
    /// - Code with state appended via `#` (code#state)
    pub async fn exchange_code(
        &self,
        code: String,
        verifier: PkceVerifier,
    ) -> Result<TokenResponse> {
        // Extract code and state from input
        let (actual_code, state) = if code.contains("code=") {
            // Parse from full callback URL: https://...?code=XXX&state=YYY
            let code_part = code
                .split("code=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow!("Could not parse code from URL"))?;

            // Also try to extract state from URL
            let state_part = code
                .split("state=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .map(|s| s.to_string());

            (code_part, state_part)
        } else if code.contains('#') {
            // Code might have state appended via # (internal format)
            let parts: Vec<&str> = code.split('#').collect();
            let code_part = parts.first().unwrap_or(&code.as_str()).to_string();
            let state_part = parts.get(1).map(|s| s.to_string());
            (code_part, state_part)
        } else {
            (code, None)
        };

        tracing::info!("Token exchange - State present: {}", state.is_some());

        // Create JSON request body (NOT form-encoded!)
        let mut request_body = serde_json::json!({
            "grant_type": "authorization_code",
            "code": actual_code,
            "client_id": CLIENT_ID,
            "redirect_uri": REDIRECT_URI,
            "code_verifier": verifier.0
        });

        // Add state if present (Anthropic may require this)
        if let Some(state_value) = &state {
            request_body["state"] = serde_json::Value::String(state_value.clone());
        }

        let response = self
            .http_client
            .post(TOKEN_ENDPOINT)
            .header("Content-Type", "application/json")
            .header("anthropic-beta", "oauth-2025-04-20") // Required header!
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            tracing::error!("Token exchange failed: {} - {}", status, error_text);
            return Err(anyhow!(
                "Token exchange failed ({}): {}",
                status,
                error_text
            ));
        }

        let tokens: TokenResponse = response.json().await?;
        tracing::info!("Successfully obtained OAuth tokens");
        Ok(tokens)
    }

    /// Refresh an expired access token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        let request_body = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID
        });

        let response = self
            .http_client
            .post(TOKEN_ENDPOINT)
            .header("Content-Type", "application/json")
            .header("anthropic-beta", "oauth-2025-04-20")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Token refresh failed: {}", error_text));
        }

        let tokens: TokenResponse = response.json().await?;
        Ok(tokens)
    }
}

impl Default for OAuthClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Start OAuth flow - returns the auth URL and verifier
///
/// The flow works as follows:
/// 1. Call this function to get auth URL and verifier
/// 2. Open the auth URL in browser (or let user open it)
/// 3. User authorizes the app
/// 4. User is redirected to console.anthropic.com/oauth/code/callback?code=...
/// 5. That page will show an error, but the URL contains the code
/// 6. User copies the full URL back into the app
/// 7. Call exchange_code() with the URL and the stored verifier
pub fn start_oauth_flow() -> (String, PkceVerifier) {
    let client = OAuthClient::new();
    client.get_auth_url()
}

/// Exchange callback URL/code for tokens
pub async fn finish_oauth_flow(callback: String, verifier: PkceVerifier) -> Result<TokenResponse> {
    let client = OAuthClient::new();
    client.exchange_code(callback, verifier).await
}
