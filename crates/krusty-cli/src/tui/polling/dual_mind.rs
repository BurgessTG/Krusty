//! Dual-mind dialogue channel polling
//!
//! Handles dialogue updates from the Big Claw / Little Claw system.
//!
//! Design: Dialogue is invisible to user except when Little Claw has
//! actual concerns. Routine approvals ("Proceed") are silent.

use crate::agent::dual_mind::extract_insight_patterns;
use crate::tui::utils::AsyncChannels;

use super::PollResult;

/// Potential insights extracted from review output (not yet saved to DB)
pub struct ExtractedInsights {
    pub insights: Vec<String>,
}

/// Poll dual-mind dialogue channel
///
/// Only surfaces actual concerns - routine approvals are silent.
/// The quality improvement happens invisibly through critique injection
/// into tool results, which Big Claw then sees and acts on.
///
/// Returns both poll result and any extracted insights that should be saved.
pub fn poll_dual_mind(channels: &mut AsyncChannels) -> (PollResult, Option<ExtractedInsights>) {
    let mut result = PollResult::new();
    let mut extracted = None;

    let Some(mut rx) = channels.dual_mind.take() else {
        return (result, extracted);
    };

    loop {
        match rx.try_recv() {
            Ok(update) => {
                // Only show enhancements (actual concerns) - not routine dialogue
                // The dialogue is logged via tracing for debugging but not shown to user
                if let Some(enhancement) = update.enhancement {
                    // Only show if it's a substantive concern, not just "Proceed"
                    let lower = enhancement.to_lowercase();
                    if !lower.contains("proceed")
                        && !lower.contains("approved")
                        && !lower.contains("looks good")
                        && !lower.contains("no issues")
                    {
                        result.needs_redraw = true;
                        // Format as a subtle system note, not a chat message
                        tracing::info!("Little Claw concern: {}", enhancement);
                    }
                }

                // Extract potential insights from review output
                // These will be saved by the caller who has database access
                if let Some(review_output) = update.review_output {
                    let insights = extract_insight_patterns(&review_output);
                    if !insights.is_empty() {
                        tracing::debug!(
                            insight_count = insights.len(),
                            "Extracted potential insights from review"
                        );
                        extracted = Some(ExtractedInsights { insights });
                    }
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                channels.dual_mind = Some(rx);
                break;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                tracing::debug!("Dual-mind dialogue channel disconnected");
                break;
            }
        }
    }

    (result, extracted)
}
