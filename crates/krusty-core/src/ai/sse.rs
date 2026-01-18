//! SSE (Server-Sent Events) stream processing utilities
//!
//! Handles parsing of SSE streams from AI providers

use bytes::Bytes;
use serde_json::Value;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::stream_buffer::StreamBuffer;
use super::streaming::StreamPart;
use super::types::{
    AiToolCall, Citation, ContextEditingMetrics, FinishReason, Usage, WebFetchContent,
    WebSearchResult,
};

/// Common SSE stream processor that handles partial lines and buffering
pub struct SseStreamProcessor {
    /// Accumulated partial line from previous chunks
    partial_line: String,
    /// Stream buffer for smooth text streaming
    stream_buffer: StreamBuffer,
    /// Channel to send processed stream parts
    tx: mpsc::UnboundedSender<StreamPart>,
    /// When the stream started
    stream_start: Instant,
    /// Event counter for logging
    event_count: usize,
    /// Bytes received counter
    bytes_received: usize,
}

impl SseStreamProcessor {
    /// Create a new SSE stream processor
    pub fn new(
        tx: mpsc::UnboundedSender<StreamPart>,
        buffer_tx: mpsc::UnboundedSender<String>,
    ) -> Self {
        info!("SSE stream processor created");
        Self {
            partial_line: String::new(),
            stream_buffer: StreamBuffer::new(buffer_tx),
            tx,
            stream_start: Instant::now(),
            event_count: 0,
            bytes_received: 0,
        }
    }

    /// Process a chunk of bytes from the SSE stream
    pub async fn process_chunk<P: SseParser>(
        &mut self,
        bytes: Bytes,
        parser: &P,
    ) -> anyhow::Result<()> {
        self.bytes_received += bytes.len();
        let text = String::from_utf8_lossy(&bytes);
        let combined = format!("{}{}", self.partial_line, text);
        debug!(
            "SSE chunk received: {} bytes (total: {} bytes)",
            bytes.len(),
            self.bytes_received
        );
        let lines: Vec<&str> = combined.lines().collect();

        // Handle partial lines
        if !combined.ends_with('\n') && !lines.is_empty() {
            self.partial_line = lines.last().unwrap_or(&"").to_string();
        } else {
            self.partial_line.clear();
        }

        // Process complete lines
        let lines_to_process = if self.partial_line.is_empty() {
            lines.len()
        } else {
            lines.len() - 1
        };

        for line in lines.iter().take(lines_to_process) {
            // Skip empty lines and SSE comments
            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            // Process SSE event line
            if let Some(data) = line.strip_prefix("data: ") {
                self.process_sse_data(data, parser).await?;
            }
        }

        Ok(())
    }

    /// Process SSE data using the provider-specific parser
    pub async fn process_sse_data<P: SseParser>(
        &mut self,
        data: &str,
        parser: &P,
    ) -> anyhow::Result<()> {
        self.event_count += 1;
        let elapsed = self.stream_start.elapsed();

        // Handle end-of-stream marker
        if data == "[DONE]" {
            info!(
                "SSE stream [DONE] marker received after {:?}, {} events, {} bytes",
                elapsed, self.event_count, self.bytes_received
            );
            self.stream_buffer.flush().await;
            let _ = self.tx.send(StreamPart::Finish {
                reason: FinishReason::Stop,
            });
            return Ok(());
        }

        // Parse JSON and convert to stream events
        if let Ok(json) = serde_json::from_str::<Value>(data) {
            // Log the raw event type for debugging
            let event_type = json
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            debug!(
                "SSE event #{} at {:?}: type={}",
                self.event_count, elapsed, event_type
            );

            match parser.parse_event(&json).await? {
                SseEvent::TextDelta(text) => {
                    debug!("  -> TextDelta: {} chars", text.len());
                    self.stream_buffer.process_chunk(text).await;
                }
                SseEvent::TextDeltaWithCitations { text, citations } => {
                    debug!(
                        "  -> TextDeltaWithCitations: {} chars, {} citations",
                        text.len(),
                        citations.len()
                    );
                    let _ = self.tx.send(StreamPart::TextDeltaWithCitations {
                        delta: text,
                        citations,
                    });
                }
                SseEvent::ToolCallStart { id, name } => {
                    info!(
                        "SSE ToolCallStart: id={}, name={} at {:?}",
                        id, name, elapsed
                    );
                    let _ = self.tx.send(StreamPart::ToolCallStart { id, name });
                }
                SseEvent::ToolCallDelta { id, delta } => {
                    debug!("  -> ToolCallDelta: id={}, {} chars", id, delta.len());
                    let _ = self.tx.send(StreamPart::ToolCallDelta { id, delta });
                }
                SseEvent::ToolCallComplete(tool_call) => {
                    info!(
                        "SSE ToolCallComplete: id={}, name={} at {:?}",
                        tool_call.id, tool_call.name, elapsed
                    );
                    let _ = self.tx.send(StreamPart::ToolCallComplete { tool_call });
                }
                // Server-executed tools
                SseEvent::ServerToolStart { id, name } => {
                    info!(
                        "SSE ServerToolStart: id={}, name={} at {:?}",
                        id, name, elapsed
                    );
                    let _ = self.tx.send(StreamPart::ServerToolStart { id, name });
                }
                SseEvent::ServerToolDelta { id, delta } => {
                    debug!("  -> ServerToolDelta: id={}, {} chars", id, delta.len());
                    let _ = self.tx.send(StreamPart::ServerToolDelta { id, delta });
                }
                SseEvent::ServerToolComplete { id, name, input } => {
                    info!(
                        "SSE ServerToolComplete: id={}, name={} at {:?}",
                        id, name, elapsed
                    );
                    let _ = self
                        .tx
                        .send(StreamPart::ServerToolComplete { id, name, input });
                }
                SseEvent::WebSearchResults {
                    tool_use_id,
                    results,
                } => {
                    info!(
                        "SSE WebSearchResults: {} results for {} at {:?}",
                        results.len(),
                        tool_use_id,
                        elapsed
                    );
                    let _ = self.tx.send(StreamPart::WebSearchResults {
                        tool_use_id,
                        results,
                    });
                }
                SseEvent::WebFetchResult {
                    tool_use_id,
                    content,
                } => {
                    info!(
                        "SSE WebFetchResult: url={} for {} at {:?}",
                        content.url, tool_use_id, elapsed
                    );
                    let _ = self.tx.send(StreamPart::WebFetchResult {
                        tool_use_id,
                        content,
                    });
                }
                SseEvent::ServerToolError {
                    tool_use_id,
                    error_code,
                } => {
                    warn!(
                        "SSE ServerToolError: {} for {} at {:?}",
                        error_code, tool_use_id, elapsed
                    );
                    let _ = self.tx.send(StreamPart::ServerToolError {
                        tool_use_id,
                        error_code,
                    });
                }
                // Extended thinking
                SseEvent::ThinkingStart { index } => {
                    info!("SSE ThinkingStart: index={} at {:?}", index, elapsed);
                    let _ = self.tx.send(StreamPart::ThinkingStart { index });
                }
                SseEvent::ThinkingDelta { index, thinking } => {
                    debug!(
                        "  -> ThinkingDelta: index={}, {} chars",
                        index,
                        thinking.len()
                    );
                    let _ = self.tx.send(StreamPart::ThinkingDelta { index, thinking });
                }
                SseEvent::SignatureDelta { index, signature } => {
                    debug!(
                        "  -> SignatureDelta: index={}, {} chars",
                        index,
                        signature.len()
                    );
                    let _ = self
                        .tx
                        .send(StreamPart::SignatureDelta { index, signature });
                }
                SseEvent::ThinkingComplete {
                    index,
                    thinking,
                    signature,
                } => {
                    info!(
                        "SSE ThinkingComplete: index={}, thinking={} chars, sig={} chars at {:?}",
                        index,
                        thinking.len(),
                        signature.len(),
                        elapsed
                    );
                    let _ = self.tx.send(StreamPart::ThinkingComplete {
                        index,
                        thinking,
                        signature,
                    });
                }
                SseEvent::Finish { reason } => {
                    info!(
                        "SSE Finish: reason={:?} at {:?} ({} events, {} bytes)",
                        reason, elapsed, self.event_count, self.bytes_received
                    );
                    self.stream_buffer.flush().await;
                    let _ = self.tx.send(StreamPart::Finish { reason });
                }
                SseEvent::Usage(usage) => {
                    info!("SSE Usage: prompt={}, completion={}, total={}, cache_read={}, cache_created={}",
                        usage.prompt_tokens, usage.completion_tokens, usage.total_tokens,
                        usage.cache_read_input_tokens, usage.cache_creation_input_tokens);
                    let _ = self.tx.send(StreamPart::Usage { usage });
                }
                SseEvent::ContextEdited(metrics) => {
                    info!(
                        "SSE ContextEdited: cleared {} tokens ({} tool uses, {} thinking turns)",
                        metrics.cleared_input_tokens,
                        metrics.cleared_tool_uses,
                        metrics.cleared_thinking_turns
                    );
                    let _ = self.tx.send(StreamPart::ContextEdited { metrics });
                }
                SseEvent::Skip => {
                    // Event should be ignored
                    debug!("  -> Skip event");
                }
            }
        } else if !data.is_empty() && !data.trim().is_empty() {
            warn!(
                "Failed to parse SSE JSON (event #{}): {}",
                self.event_count, data
            );
        }

        Ok(())
    }

    /// Finish processing and ensure all buffers are flushed
    pub async fn finish(&mut self) {
        let elapsed = self.stream_start.elapsed();
        info!(
            "SSE stream processor finishing: {:?} elapsed, {} events, {} bytes total",
            elapsed, self.event_count, self.bytes_received
        );
        self.stream_buffer.finish().await;
    }
}

/// Events that can be parsed from SSE data
pub enum SseEvent {
    TextDelta(String),
    TextDeltaWithCitations {
        text: String,
        citations: Vec<Citation>,
    },
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallDelta {
        id: String,
        delta: String,
    },
    ToolCallComplete(AiToolCall),
    // Server-executed tools (web_search, web_fetch)
    ServerToolStart {
        id: String,
        name: String,
    },
    ServerToolDelta {
        id: String,
        delta: String,
    },
    ServerToolComplete {
        id: String,
        name: String,
        input: Value,
    },
    WebSearchResults {
        tool_use_id: String,
        results: Vec<WebSearchResult>,
    },
    WebFetchResult {
        tool_use_id: String,
        content: WebFetchContent,
    },
    ServerToolError {
        tool_use_id: String,
        error_code: String,
    },
    // Extended thinking
    ThinkingStart {
        index: usize,
    },
    ThinkingDelta {
        index: usize,
        thinking: String,
    },
    SignatureDelta {
        index: usize,
        signature: String,
    },
    ThinkingComplete {
        index: usize,
        thinking: String,
        signature: String,
    },
    Finish {
        reason: FinishReason,
    },
    Usage(Usage),
    ContextEdited(ContextEditingMetrics),
    Skip,
}

/// Trait for provider-specific SSE parsing logic
#[async_trait::async_trait]
pub trait SseParser: Send + Sync {
    /// Parse a JSON event into an SSE event
    async fn parse_event(&self, json: &Value) -> anyhow::Result<SseEvent>;
}

/// Common helper to parse finish reasons
pub fn parse_finish_reason(reason_str: &str) -> FinishReason {
    match reason_str {
        "stop" | "end_turn" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolCalls,
        _ => FinishReason::Other(reason_str.to_string()),
    }
}

/// Create standard streaming channels with buffer processing
pub fn create_streaming_channels() -> (
    mpsc::UnboundedSender<StreamPart>,
    mpsc::UnboundedReceiver<StreamPart>,
    mpsc::UnboundedSender<String>,
    mpsc::UnboundedReceiver<String>,
) {
    let (tx, rx) = mpsc::unbounded_channel::<StreamPart>();
    let (buffer_tx, buffer_rx) = mpsc::unbounded_channel::<String>();
    (tx, rx, buffer_tx, buffer_rx)
}

/// Spawn a task to convert buffered text into StreamParts
pub fn spawn_buffer_processor(
    mut buffer_rx: mpsc::UnboundedReceiver<String>,
    tx: mpsc::UnboundedSender<StreamPart>,
) {
    tokio::spawn(async move {
        while let Some(text) = buffer_rx.recv().await {
            let _ = tx.send(StreamPart::TextDelta { delta: text });
        }
    });
}

/// Tool call accumulator for providers that stream tool calls in parts
#[derive(Debug, Clone)]
pub struct ToolCallAccumulator {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub is_complete: bool,
}

/// Server tool accumulator for web_search/web_fetch
#[derive(Debug, Clone)]
pub struct ServerToolAccumulator {
    pub id: String,
    pub name: String,
    pub input_json: String,
    pub is_complete: bool,
}

impl ServerToolAccumulator {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            input_json: String::new(),
            is_complete: false,
        }
    }

    pub fn add_input(&mut self, delta: &str) {
        self.input_json.push_str(delta);
    }

    pub fn complete(&mut self) -> Value {
        self.is_complete = true;
        if self.input_json.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str::<Value>(&self.input_json)
                .unwrap_or_else(|_| serde_json::json!({"raw": self.input_json.clone()}))
        }
    }
}

/// Thinking block accumulator for extended thinking
#[derive(Debug, Clone)]
pub struct ThinkingAccumulator {
    pub thinking: String,
    pub signature: String,
    pub is_complete: bool,
}

impl ThinkingAccumulator {
    pub fn new() -> Self {
        Self {
            thinking: String::new(),
            signature: String::new(),
            is_complete: false,
        }
    }

    pub fn add_thinking(&mut self, delta: &str) {
        self.thinking.push_str(delta);
    }

    pub fn add_signature(&mut self, delta: &str) {
        self.signature.push_str(delta);
    }

    pub fn complete(&mut self) -> (String, String) {
        self.is_complete = true;
        (self.thinking.clone(), self.signature.clone())
    }
}

impl Default for ThinkingAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallAccumulator {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            arguments: String::new(),
            is_complete: false,
        }
    }

    pub fn add_arguments(&mut self, delta: &str) {
        self.arguments.push_str(delta);
    }

    pub fn try_complete(&mut self) -> Option<AiToolCall> {
        if !self.arguments.is_empty() {
            if let Ok(parsed) = serde_json::from_str::<Value>(&self.arguments) {
                self.is_complete = true;
                return Some(AiToolCall {
                    id: self.id.clone(),
                    name: self.name.clone(),
                    arguments: parsed,
                });
            }
        }
        None
    }

    pub fn force_complete(&mut self) -> AiToolCall {
        self.is_complete = true;
        AiToolCall {
            id: self.id.clone(),
            name: self.name.clone(),
            arguments: if self.arguments.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str::<Value>(&self.arguments)
                    .unwrap_or_else(|_| serde_json::json!({"raw": self.arguments.clone()}))
            },
        }
    }
}
