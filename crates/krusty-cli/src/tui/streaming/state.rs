//! Streaming State Machine
//!
//! This module provides a proper state machine for streaming responses,
//! replacing the fragile flag-based approach (pending_tool_calls,
//! pending_thinking_blocks, stream_complete_handled, stream_finished).

use crate::ai::streaming::StreamPart;
use crate::ai::types::{
    AiToolCall, Citation, Content, ModelMessage, Role, WebFetchContent, WebSearchResult,
};
use tokio::sync::mpsc;

/// A thinking block with its signature (for extended thinking)
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    pub thinking: String,
    pub signature: String,
}

/// The streaming state machine
///
/// Replaces four separate flags with a single enum that explicitly
/// tracks all possible states and their associated data.
#[derive(Debug, Default)]
pub enum StreamPhase {
    /// No stream active
    #[default]
    Idle,

    /// Actively receiving stream events
    Receiving {
        /// Channel receiving stream parts
        rx: mpsc::UnboundedReceiver<StreamPart>,
        /// Accumulated text from TextDelta events
        text_buffer: String,
        /// Accumulated thinking blocks
        thinking_blocks: Vec<ThinkingBlock>,
        /// Accumulated tool calls
        tool_calls: Vec<AiToolCall>,
        /// True when Finished event received (all API events delivered)
        api_finished: bool,
    },

    /// Stream complete, tools ready to execute
    /// This state exists between stream completion and tool execution
    ReadyForTools {
        /// The accumulated text (will be part of assistant message)
        text: String,
        /// Thinking blocks to include in message
        thinking_blocks: Vec<ThinkingBlock>,
        /// Tool calls to execute
        tool_calls: Vec<AiToolCall>,
    },

    /// Stream completed with content (text/thinking, no tools)
    /// Content is preserved here so build_assistant_message can access it
    CompleteWithContent {
        /// The accumulated text
        text: String,
        /// Thinking blocks
        thinking_blocks: Vec<ThinkingBlock>,
    },

    /// Stream completed and message already built/saved
    Complete,
}

/// Events emitted during streaming (same as before, but used by StreamingManager)
#[derive(Debug)]
pub enum StreamEvent {
    /// New text content
    TextDelta { delta: String },
    /// Text content with citations
    TextDeltaWithCitations {
        delta: String,
        citations: Vec<Citation>,
    },
    /// Tool call started (client-side)
    ToolStart { name: String },
    /// Tool call delta (partial arguments) - currently ignored
    ToolDelta,
    /// Tool call completed (client-side)
    ToolComplete { call: AiToolCall },
    /// Server tool started (web_search, web_fetch)
    ServerToolStart { id: String, name: String },
    /// Server tool delta - currently ignored
    ServerToolDelta,
    /// Server tool completed
    ServerToolComplete { id: String, name: String },
    /// Web search results received
    WebSearchResults {
        tool_use_id: String,
        results: Vec<WebSearchResult>,
    },
    /// Web fetch result received
    WebFetchResult {
        tool_use_id: String,
        content: WebFetchContent,
    },
    /// Server tool error
    ServerToolError {
        tool_use_id: String,
        error_code: String,
    },
    /// Thinking block started (extended thinking)
    ThinkingStart,
    /// Thinking content delta
    ThinkingDelta { thinking: String },
    /// Thinking block complete (thinking content stored in ThinkingBlock)
    ThinkingComplete { signature: String },
    /// API finished sending events (Finished event received)
    Finished {
        reason: crate::ai::types::FinishReason,
    },
    /// Channel closed - stream fully complete (Complete event)
    Complete { text: String },
    /// Error occurred
    Error { error: String },
    /// Token usage (with cache metrics)
    Usage {
        prompt_tokens: usize,
        completion_tokens: usize,
        cache_read_tokens: usize,
        cache_created_tokens: usize,
    },
    /// Context was edited by server (old content cleared)
    ContextEdited {
        cleared_tokens: usize,
        cleared_tool_uses: usize,
        cleared_thinking_turns: usize,
    },
}

/// Manages streaming state with explicit state machine
pub struct StreamingManager {
    phase: StreamPhase,
}

impl StreamingManager {
    pub fn new() -> Self {
        Self {
            phase: StreamPhase::Idle,
        }
    }

    /// Start a new streaming session
    pub fn start_stream(&mut self, rx: mpsc::UnboundedReceiver<StreamPart>) {
        tracing::info!("StreamingManager: starting stream");
        self.phase = StreamPhase::Receiving {
            rx,
            text_buffer: String::new(),
            thinking_blocks: Vec::new(),
            tool_calls: Vec::new(),
            api_finished: false,
        };
    }

    /// Check if tools are ready to execute
    pub fn is_ready_for_tools(&self) -> bool {
        matches!(self.phase, StreamPhase::ReadyForTools { .. })
    }

    /// Get current phase for debugging
    pub fn phase_name(&self) -> &'static str {
        match &self.phase {
            StreamPhase::Idle => "Idle",
            StreamPhase::Receiving { .. } => "Receiving",
            StreamPhase::ReadyForTools { .. } => "ReadyForTools",
            StreamPhase::CompleteWithContent { .. } => "CompleteWithContent",
            StreamPhase::Complete => "Complete",
        }
    }

    /// Poll for next stream event
    ///
    /// Returns None if no event available (channel empty or not streaming)
    pub fn poll(&mut self) -> Option<StreamEvent> {
        // Only poll in Receiving state
        let StreamPhase::Receiving {
            rx,
            text_buffer,
            thinking_blocks,
            tool_calls,
            api_finished,
        } = &mut self.phase
        else {
            return None;
        };

        match rx.try_recv() {
            Ok(part) => {
                let event = self.process_part(part);
                Some(event)
            }
            Err(mpsc::error::TryRecvError::Empty) => None,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // Channel closed - finalize the stream
                tracing::info!(
                    "StreamingManager: channel closed, text_len={}, thinking_blocks={}, tool_calls={}, api_finished={}",
                    text_buffer.len(),
                    thinking_blocks.len(),
                    tool_calls.len(),
                    *api_finished
                );

                // Take ownership of accumulated state
                let text = std::mem::take(text_buffer);
                let blocks = std::mem::take(thinking_blocks);
                let calls = std::mem::take(tool_calls);

                // Capture text BEFORE transitioning (it gets lost in Complete phase)
                let complete_text = text.clone();

                // Transition to appropriate next state
                if calls.is_empty() {
                    // Store content in Complete phase so build_assistant_message can access it
                    self.phase = StreamPhase::CompleteWithContent {
                        text,
                        thinking_blocks: blocks,
                    };
                } else {
                    self.phase = StreamPhase::ReadyForTools {
                        text,
                        thinking_blocks: blocks,
                        tool_calls: calls,
                    };
                }

                Some(StreamEvent::Complete {
                    text: complete_text,
                })
            }
        }
    }

    /// Process a stream part and update internal state
    fn process_part(&mut self, part: StreamPart) -> StreamEvent {
        // Must be in Receiving state
        let StreamPhase::Receiving {
            text_buffer,
            thinking_blocks,
            tool_calls,
            api_finished,
            ..
        } = &mut self.phase
        else {
            return StreamEvent::Error {
                error: "Not in receiving state".to_string(),
            };
        };

        match part {
            StreamPart::TextDelta { delta } => {
                text_buffer.push_str(&delta);
                StreamEvent::TextDelta { delta }
            }
            StreamPart::TextDeltaWithCitations { delta, citations } => {
                text_buffer.push_str(&delta);
                StreamEvent::TextDeltaWithCitations { delta, citations }
            }
            StreamPart::ToolCallStart { name, .. } => StreamEvent::ToolStart { name },
            StreamPart::ToolCallDelta { .. } => StreamEvent::ToolDelta,
            StreamPart::ToolCallComplete { tool_call } => {
                tracing::info!("StreamingManager: tool call complete - {}", tool_call.name);
                tool_calls.push(tool_call.clone());
                StreamEvent::ToolComplete { call: tool_call }
            }
            StreamPart::ServerToolStart { id, name } => StreamEvent::ServerToolStart { id, name },
            StreamPart::ServerToolDelta { .. } => StreamEvent::ServerToolDelta,
            StreamPart::ServerToolComplete { id, name, .. } => {
                StreamEvent::ServerToolComplete { id, name }
            }
            StreamPart::WebSearchResults {
                tool_use_id,
                results,
            } => StreamEvent::WebSearchResults {
                tool_use_id,
                results,
            },
            StreamPart::WebFetchResult {
                tool_use_id,
                content,
            } => StreamEvent::WebFetchResult {
                tool_use_id,
                content,
            },
            StreamPart::ServerToolError {
                tool_use_id,
                error_code,
            } => StreamEvent::ServerToolError {
                tool_use_id,
                error_code,
            },
            StreamPart::ThinkingStart { .. } => StreamEvent::ThinkingStart,
            StreamPart::ThinkingDelta { thinking, .. } => StreamEvent::ThinkingDelta { thinking },
            StreamPart::SignatureDelta { .. } => {
                // Signature deltas don't need to be displayed
                StreamEvent::TextDelta {
                    delta: String::new(),
                }
            }
            StreamPart::ThinkingComplete {
                thinking,
                signature,
                ..
            } => {
                tracing::info!(
                    "StreamingManager: thinking complete, signature_len={}",
                    signature.len()
                );
                thinking_blocks.push(ThinkingBlock {
                    thinking,
                    signature: signature.clone(),
                });
                StreamEvent::ThinkingComplete { signature }
            }
            StreamPart::Finish { reason } => {
                tracing::info!("StreamingManager: API finished, reason={:?}", reason);
                *api_finished = true;
                StreamEvent::Finished { reason }
            }
            StreamPart::Error { error } => {
                tracing::error!("StreamingManager: stream error - {}", error);
                StreamEvent::Error { error }
            }
            StreamPart::Usage { usage } => StreamEvent::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                cache_read_tokens: usage.cache_read_input_tokens,
                cache_created_tokens: usage.cache_creation_input_tokens,
            },
            StreamPart::ContextEdited { metrics } => StreamEvent::ContextEdited {
                cleared_tokens: metrics.cleared_input_tokens,
                cleared_tool_uses: metrics.cleared_tool_uses,
                cleared_thinking_turns: metrics.cleared_thinking_turns,
            },
            StreamPart::Start { .. } => {
                // Skip start events
                StreamEvent::TextDelta {
                    delta: String::new(),
                }
            }
        }
    }

    /// Take pending tool calls for execution (transitions to Complete)
    ///
    /// Returns None if not in ReadyForTools state
    pub fn take_tool_calls(&mut self) -> Option<(String, Vec<ThinkingBlock>, Vec<AiToolCall>)> {
        // Use a temporary Idle to swap
        let old_phase = std::mem::replace(&mut self.phase, StreamPhase::Idle);

        match old_phase {
            StreamPhase::ReadyForTools {
                text,
                thinking_blocks,
                tool_calls,
            } => {
                tracing::info!(
                    "StreamingManager: taking {} tool calls for execution",
                    tool_calls.len()
                );
                self.phase = StreamPhase::Complete;
                Some((text, thinking_blocks, tool_calls))
            }
            other => {
                // Restore original state
                self.phase = other;
                None
            }
        }
    }

    /// Build the assistant message from accumulated state
    ///
    /// Call this when transitioning out of Receiving, ReadyForTools, or CompleteWithContent
    pub fn build_assistant_message(&self) -> Option<ModelMessage> {
        let (text, thinking_blocks, tool_calls): (&str, &[ThinkingBlock], &[AiToolCall]) =
            match &self.phase {
                StreamPhase::Receiving {
                    text_buffer,
                    thinking_blocks,
                    tool_calls,
                    ..
                } => (
                    text_buffer.as_str(),
                    thinking_blocks.as_slice(),
                    tool_calls.as_slice(),
                ),
                StreamPhase::ReadyForTools {
                    text,
                    thinking_blocks,
                    tool_calls,
                } => (
                    text.as_str(),
                    thinking_blocks.as_slice(),
                    tool_calls.as_slice(),
                ),
                StreamPhase::CompleteWithContent {
                    text,
                    thinking_blocks,
                } => (text.as_str(), thinking_blocks.as_slice(), &[]),
                _ => return None,
            };

        let mut content = Vec::new();

        // Add thinking blocks first
        for block in thinking_blocks {
            content.push(Content::Thinking {
                thinking: block.thinking.clone(),
                signature: block.signature.clone(),
            });
        }

        // Add text if present
        if !text.is_empty() {
            content.push(Content::Text {
                text: text.to_string(),
            });
        }

        // Add tool uses
        for call in tool_calls {
            content.push(Content::ToolUse {
                id: call.id.clone(),
                name: call.name.clone(),
                input: call.arguments.clone(),
            });
        }

        if content.is_empty() {
            return None;
        }

        Some(ModelMessage {
            role: Role::Assistant,
            content,
        })
    }

    /// Get concatenated thinking block text from current phase
    ///
    /// Returns thinking content when in CompleteWithContent or ReadyForTools phase,
    /// allowing completion detection to check reasoning blocks where models often
    /// mention task completions.
    pub fn thinking_text(&self) -> Option<String> {
        let blocks = match &self.phase {
            StreamPhase::CompleteWithContent {
                thinking_blocks, ..
            } => thinking_blocks,
            StreamPhase::ReadyForTools {
                thinking_blocks, ..
            } => thinking_blocks,
            _ => return None,
        };
        if blocks.is_empty() {
            return None;
        }
        Some(
            blocks
                .iter()
                .map(|b| b.thinking.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// Reset to idle state
    pub fn reset(&mut self) {
        tracing::info!("StreamingManager: resetting to Idle");
        self.phase = StreamPhase::Idle;
    }
}

impl Default for StreamingManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let manager = StreamingManager::new();
        assert_eq!(manager.phase_name(), "Idle");
        assert!(!manager.is_ready_for_tools());
    }

    #[test]
    fn test_phase_names() {
        let manager = StreamingManager::new();
        assert_eq!(manager.phase_name(), "Idle");

        // We can't easily test other states without channels,
        // but this verifies the basic structure works
    }
}
