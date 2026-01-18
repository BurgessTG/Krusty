//! Token management with automatic refresh
//!
//! Handles storage and automatic refresh of OAuth tokens

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::oauth::{OAuthClient, TokenResponse};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
    pub token_type: String,
    pub scope: String,
    /// API key created from OAuth token - this is what we actually use for API calls
    #[serde(default)]
    pub api_key: Option<String>,
}

impl From<TokenResponse> for StoredTokens {
    fn from(response: TokenResponse) -> Self {
        // Calculate expiration time (subtract 5 minutes buffer)
        let expires_at = Utc::now() + Duration::seconds(response.expires_in as i64 - 300);

        StoredTokens {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            expires_at,
            token_type: response.token_type,
            scope: response.scope,
            api_key: None,
        }
    }
}

pub struct TokenManager {
    oauth_client: OAuthClient,
    tokens: Arc<RwLock<Option<StoredTokens>>>,
    storage_path: PathBuf,
}

impl TokenManager {
    pub async fn new(storage_path: PathBuf) -> Result<Self> {
        info!("TokenManager: Initializing with path: {:?}", storage_path);
        let oauth_client = OAuthClient::new();

        // Try to load existing tokens
        let tokens = if storage_path.exists() {
            info!("TokenManager: Token file exists at {:?}", storage_path);
            match fs::read_to_string(&storage_path).await {
                Ok(content) => {
                    debug!(
                        "TokenManager: Read token file content length: {}",
                        content.len()
                    );
                    match serde_json::from_str::<StoredTokens>(&content) {
                        Ok(tokens) => {
                            info!("TokenManager: Successfully loaded tokens");
                            Some(tokens)
                        }
                        Err(e) => {
                            error!("TokenManager: Failed to parse stored tokens: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    error!("TokenManager: Failed to read token file: {}", e);
                    None
                }
            }
        } else {
            warn!(
                "TokenManager: Token file does not exist at {:?}",
                storage_path
            );
            None
        };

        Ok(Self {
            oauth_client,
            tokens: Arc::new(RwLock::new(tokens)),
            storage_path,
        })
    }

    /// Get a valid access token, automatically refreshing if needed
    pub async fn get_valid_token(&self) -> Result<String> {
        debug!(
            "TokenManager: Getting valid token from path: {:?}",
            self.storage_path
        );
        let mut tokens_guard = self.tokens.write().await;

        match &*tokens_guard {
            None => {
                error!(
                    "TokenManager: No tokens in memory for path: {:?}",
                    self.storage_path
                );
                Err(anyhow!("No tokens available. Please authenticate first."))
            }
            Some(tokens) => {
                // Check if token is expired or about to expire
                if Utc::now() >= tokens.expires_at {
                    // Token expired, refresh it
                    debug!("Access token expired, refreshing...");

                    let new_tokens = self
                        .oauth_client
                        .refresh_token(&tokens.refresh_token)
                        .await?;

                    let stored_tokens = StoredTokens::from(new_tokens);

                    // Save to disk
                    self.save_tokens(&stored_tokens).await?;

                    // Update in memory
                    *tokens_guard = Some(stored_tokens.clone());

                    Ok(stored_tokens.access_token)
                } else {
                    // Token still valid
                    Ok(tokens.access_token.clone())
                }
            }
        }
    }

    /// Save tokens to disk with proper permissions
    async fn save_tokens(&self, tokens: &StoredTokens) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Serialize tokens
        let json = serde_json::to_string_pretty(tokens)?;

        // Write to file
        fs::write(&self.storage_path, json).await?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&self.storage_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(&self.storage_path, permissions).await?;
        }

        Ok(())
    }
}
