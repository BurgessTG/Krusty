//! OAuth status channel polling
//!
//! Handles status updates from background OAuth authentication tasks.

use krusty_core::ai::providers::ProviderId;
use krusty_core::auth::{OAuthTokenData, OAuthTokenStore};

use crate::tui::popups::auth::AuthPopup;
use crate::tui::utils::AsyncChannels;

use super::PollResult;

/// Poll OAuth status updates from background authentication tasks
#[allow(dead_code)]
pub fn poll_oauth_status(
    channels: &mut AsyncChannels,
    auth_popup: &mut AuthPopup,
    active_provider: ProviderId,
    mut switch_provider: impl FnMut(ProviderId),
) -> PollResult {
    let mut result = PollResult::new();

    let Some(mut rx) = channels.oauth_status.take() else {
        return result;
    };

    loop {
        match rx.try_recv() {
            Ok(update) => {
                result.needs_redraw = true;

                // Handle device code info (show to user)
                if let Some(device_info) = &update.device_code {
                    auth_popup
                        .set_device_code(&device_info.user_code, &device_info.verification_uri);
                }

                if update.success {
                    // Save the OAuth token
                    if let Some(token) = update.token {
                        if let Err(e) = save_oauth_token(update.provider, token) {
                            tracing::error!("Failed to save OAuth token: {}", e);
                            auth_popup.set_oauth_error(&format!("Failed to save token: {}", e));
                        } else {
                            // Mark auth as complete
                            auth_popup.set_oauth_complete();

                            // Switch to the authenticated provider
                            if active_provider != update.provider {
                                switch_provider(update.provider);
                            }

                            result = result.with_message(
                                "system",
                                format!("{} authenticated via OAuth!", update.provider),
                            );
                        }
                    }
                } else {
                    // Show error
                    auth_popup.set_oauth_error(&update.message);
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                channels.oauth_status = Some(rx);
                break;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                break;
            }
        }
    }

    result
}

/// Save OAuth token to storage
#[allow(dead_code)]
fn save_oauth_token(provider: ProviderId, token: OAuthTokenData) -> anyhow::Result<()> {
    let mut store = OAuthTokenStore::load()?;
    store.set(provider, token);
    store.save()?;
    Ok(())
}
