//! Anthropic-specific SSE parser

use anyhow::Result;
use serde_json::Value;
use tracing::info;

use crate::ai::sse::{
    parse_finish_reason, ServerToolAccumulator, SseEvent, SseParser, ThinkingAccumulator,
    ToolCallAccumulator,
};
use crate::ai::types::{
    Citation, ContextEditingMetrics, FinishReason, Usage, WebFetchContent, WebSearchResult,
};

/// Anthropic-specific SSE parser
pub struct AnthropicParser {
    /// Track tool calls by content block index
    tool_accumulators: std::sync::Mutex<std::collections::HashMap<usize, ToolCallAccumulator>>,
    /// Track thinking blocks by content block index
    thinking_accumulators: std::sync::Mutex<std::collections::HashMap<usize, ThinkingAccumulator>>,
    /// Track server tool uses by content block index
    server_tool_accumulators:
        std::sync::Mutex<std::collections::HashMap<usize, ServerToolAccumulator>>,
}

impl AnthropicParser {
    pub fn new() -> Self {
        Self {
            tool_accumulators: std::sync::Mutex::new(std::collections::HashMap::new()),
            thinking_accumulators: std::sync::Mutex::new(std::collections::HashMap::new()),
            server_tool_accumulators: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SseParser for AnthropicParser {
    async fn parse_event(&self, json: &Value) -> Result<SseEvent> {
        let event_type = json.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "content_block_start" => {
                let index = json.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                if let Some(content_block) = json.get("content_block") {
                    let block_type = content_block.get("type").and_then(|t| t.as_str());

                    match block_type {
                        Some("tool_use") => {
                            let id = content_block
                                .get("id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string();
                            let name = content_block
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();

                            // Store accumulator by index
                            let mut accumulators = self.tool_accumulators.lock().unwrap();
                            accumulators
                                .insert(index, ToolCallAccumulator::new(id.clone(), name.clone()));

                            return Ok(SseEvent::ToolCallStart { id, name });
                        }
                        Some("server_tool_use") => {
                            // Server-executed tool (web_search, web_fetch)
                            let id = content_block
                                .get("id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string();
                            let name = content_block
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();

                            let mut accumulators = self.server_tool_accumulators.lock().unwrap();
                            accumulators.insert(
                                index,
                                ServerToolAccumulator::new(id.clone(), name.clone()),
                            );

                            return Ok(SseEvent::ServerToolStart { id, name });
                        }
                        Some("web_search_tool_result") => {
                            // Parse search results immediately
                            let tool_use_id = content_block
                                .get("tool_use_id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string();

                            let results = self.parse_search_results(content_block);
                            return Ok(SseEvent::WebSearchResults {
                                tool_use_id,
                                results,
                            });
                        }
                        Some("web_fetch_tool_result") => {
                            // Parse fetch result immediately
                            let tool_use_id = content_block
                                .get("tool_use_id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string();

                            if let Some(content) = self.parse_fetch_result(content_block) {
                                return Ok(SseEvent::WebFetchResult {
                                    tool_use_id,
                                    content,
                                });
                            }

                            // Check for error
                            if let Some(err_content) = content_block.get("content") {
                                if let Some(err_type) =
                                    err_content.get("type").and_then(|t| t.as_str())
                                {
                                    if err_type == "web_fetch_tool_error"
                                        || err_type == "web_search_tool_result_error"
                                    {
                                        let error_code = err_content
                                            .get("error_code")
                                            .and_then(|e| e.as_str())
                                            .unwrap_or("unknown")
                                            .to_string();
                                        return Ok(SseEvent::ServerToolError {
                                            tool_use_id,
                                            error_code,
                                        });
                                    }
                                }
                            }
                        }
                        Some("thinking") => {
                            // Start tracking thinking block
                            let mut accumulators = self.thinking_accumulators.lock().unwrap();
                            accumulators.insert(index, ThinkingAccumulator::new());
                            return Ok(SseEvent::ThinkingStart { index });
                        }
                        _ => {}
                    }
                }
                Ok(SseEvent::Skip)
            }

            "content_block_delta" => {
                let index = json.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                if let Some(delta) = json.get("delta") {
                    let delta_type = delta.get("type").and_then(|t| t.as_str());

                    match delta_type {
                        Some("text_delta") => {
                            let text = delta
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();

                            // Check for citations
                            if let Some(citations_arr) =
                                delta.get("citations").and_then(|c| c.as_array())
                            {
                                let citations = self.parse_citations(citations_arr);
                                if !citations.is_empty() {
                                    return Ok(SseEvent::TextDeltaWithCitations {
                                        text,
                                        citations,
                                    });
                                }
                            }
                            return Ok(SseEvent::TextDelta(text));
                        }
                        Some("input_json_delta") => {
                            let partial_json = delta
                                .get("partial_json")
                                .and_then(|p| p.as_str())
                                .unwrap_or("")
                                .to_string();

                            // Check server tool accumulator first
                            {
                                let mut accumulators =
                                    self.server_tool_accumulators.lock().unwrap();
                                if let Some(acc) = accumulators.get_mut(&index) {
                                    acc.add_input(&partial_json);
                                    return Ok(SseEvent::ServerToolDelta {
                                        id: acc.id.clone(),
                                        delta: partial_json,
                                    });
                                }
                            }

                            // Then check client tool accumulator
                            let mut accumulators = self.tool_accumulators.lock().unwrap();
                            if let Some(acc) = accumulators.get_mut(&index) {
                                acc.add_arguments(&partial_json);
                                return Ok(SseEvent::ToolCallDelta {
                                    id: acc.id.clone(),
                                    delta: partial_json,
                                });
                            }
                        }
                        Some("thinking_delta") => {
                            let thinking = delta
                                .get("thinking")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();

                            // Update thinking accumulator
                            let mut accumulators = self.thinking_accumulators.lock().unwrap();
                            if let Some(acc) = accumulators.get_mut(&index) {
                                acc.add_thinking(&thinking);
                            }
                            return Ok(SseEvent::ThinkingDelta { index, thinking });
                        }
                        Some("signature_delta") => {
                            let signature = delta
                                .get("signature")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();

                            // Update thinking accumulator signature
                            let mut accumulators = self.thinking_accumulators.lock().unwrap();
                            if let Some(acc) = accumulators.get_mut(&index) {
                                acc.add_signature(&signature);
                            }
                            return Ok(SseEvent::SignatureDelta { index, signature });
                        }
                        _ => {}
                    }
                }
                Ok(SseEvent::Skip)
            }

            "content_block_stop" => {
                let index = json.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                // Check for completed server tool
                {
                    let mut accumulators = self.server_tool_accumulators.lock().unwrap();
                    if let Some(mut acc) = accumulators.remove(&index) {
                        let input = acc.complete();
                        return Ok(SseEvent::ServerToolComplete {
                            id: acc.id,
                            name: acc.name,
                            input,
                        });
                    }
                }

                // Check for completed client tool call
                {
                    let mut accumulators = self.tool_accumulators.lock().unwrap();
                    if let Some(mut acc) = accumulators.remove(&index) {
                        if let Some(tool_call) = acc.try_complete() {
                            return Ok(SseEvent::ToolCallComplete(tool_call));
                        } else {
                            // Force complete if JSON is incomplete
                            return Ok(SseEvent::ToolCallComplete(acc.force_complete()));
                        }
                    }
                }

                // Check for completed thinking block
                {
                    let mut accumulators = self.thinking_accumulators.lock().unwrap();
                    if let Some(mut acc) = accumulators.remove(&index) {
                        let (thinking, signature) = acc.complete();
                        return Ok(SseEvent::ThinkingComplete {
                            index,
                            thinking,
                            signature,
                        });
                    }
                }

                Ok(SseEvent::Skip)
            }

            "message_delta" => {
                // Check usage FIRST (message_delta contains final token counts)
                // Must check before stop_reason since both can be in same event
                if let Some(usage) = json.get("usage") {
                    let input_tokens = usage
                        .get("input_tokens")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0) as usize;
                    let output_tokens = usage
                        .get("output_tokens")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0) as usize;

                    // Only emit Usage if we have actual token data
                    if input_tokens > 0 || output_tokens > 0 {
                        let cache_read = usage
                            .get("cache_read_input_tokens")
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0) as usize;
                        let cache_creation = usage
                            .get("cache_creation_input_tokens")
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0) as usize;
                        return Ok(SseEvent::Usage(Usage {
                            prompt_tokens: input_tokens,
                            completion_tokens: output_tokens,
                            total_tokens: input_tokens + output_tokens,
                            cache_creation_input_tokens: cache_creation,
                            cache_read_input_tokens: cache_read,
                        }));
                    }
                }

                // Then check for stop_reason (Finish comes from message_stop anyway)
                if let Some(delta) = json.get("delta") {
                    if let Some(stop_reason) = delta.get("stop_reason").and_then(|s| s.as_str()) {
                        let reason = parse_finish_reason(stop_reason);
                        return Ok(SseEvent::Finish {
                            reason,
                            usage: None,
                        });
                    }
                }

                Ok(SseEvent::Skip)
            }

            "message_start" => {
                if let Some(message) = json.get("message") {
                    // Parse context editing metrics first
                    if let Some(ctx_edit) = message.get("context_editing") {
                        let metrics = ContextEditingMetrics {
                            cleared_tool_uses: ctx_edit
                                .get("cleared_tool_uses")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize,
                            cleared_thinking_turns: ctx_edit
                                .get("cleared_thinking_turns")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize,
                            cleared_input_tokens: ctx_edit
                                .get("cleared_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize,
                        };
                        if metrics.cleared_input_tokens > 0 {
                            info!("Context edited: cleared {} tokens ({} tool uses, {} thinking turns)",
                                metrics.cleared_input_tokens,
                                metrics.cleared_tool_uses,
                                metrics.cleared_thinking_turns);
                        }
                        return Ok(SseEvent::ContextEdited(metrics));
                    }

                    if let Some(usage) = message.get("usage") {
                        let input_tokens = usage
                            .get("input_tokens")
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0) as usize;
                        let cache_creation = usage
                            .get("cache_creation_input_tokens")
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0) as usize;
                        let cache_read = usage
                            .get("cache_read_input_tokens")
                            .and_then(|t| t.as_u64())
                            .unwrap_or(0) as usize;

                        // Log cache metrics
                        if cache_creation > 0 || cache_read > 0 {
                            info!(
                                "Cache metrics: read={}, created={}, fresh={}",
                                cache_read, cache_creation, input_tokens
                            );
                        }

                        let total_input = input_tokens + cache_creation + cache_read;
                        return Ok(SseEvent::Usage(Usage {
                            prompt_tokens: total_input,
                            completion_tokens: 0,
                            total_tokens: total_input,
                            cache_creation_input_tokens: cache_creation,
                            cache_read_input_tokens: cache_read,
                        }));
                    }
                }
                Ok(SseEvent::Skip)
            }

            "message_stop" => Ok(SseEvent::Finish {
                reason: FinishReason::Stop,
                usage: None,
            }),

            "error" => {
                let error_msg = json
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error");
                Err(anyhow::anyhow!("API error: {}", error_msg))
            }

            _ => Ok(SseEvent::Skip),
        }
    }
}

impl Default for AnthropicParser {
    fn default() -> Self {
        Self::new()
    }
}

// Helper methods for parsing web search/fetch results
impl AnthropicParser {
    /// Parse web search results from content block
    fn parse_search_results(&self, content_block: &Value) -> Vec<WebSearchResult> {
        let mut results = Vec::new();

        if let Some(content_arr) = content_block.get("content").and_then(|c| c.as_array()) {
            for item in content_arr {
                if item.get("type").and_then(|t| t.as_str()) == Some("web_search_result") {
                    let url = item
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = item
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let encrypted_content = item
                        .get("encrypted_content")
                        .and_then(|e| e.as_str())
                        .map(|s| s.to_string());
                    let page_age = item
                        .get("page_age")
                        .and_then(|p| p.as_str())
                        .map(|s| s.to_string());

                    results.push(WebSearchResult {
                        url,
                        title,
                        encrypted_content,
                        page_age,
                    });
                }
            }
        }

        results
    }

    /// Parse web fetch result from content block
    fn parse_fetch_result(&self, content_block: &Value) -> Option<WebFetchContent> {
        let content = content_block.get("content")?;

        // Check if it's a web_fetch_result
        if content.get("type").and_then(|t| t.as_str()) != Some("web_fetch_result") {
            return None;
        }

        let url = content
            .get("url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        let retrieved_at = content
            .get("retrieved_at")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());

        // Parse document content
        if let Some(doc) = content.get("content") {
            let title = doc
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());

            if let Some(source) = doc.get("source") {
                let media_type = source
                    .get("media_type")
                    .and_then(|m| m.as_str())
                    .unwrap_or("text/plain")
                    .to_string();

                // Get content based on source type
                let content_data = if source.get("type").and_then(|t| t.as_str()) == Some("base64")
                {
                    source
                        .get("data")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string()
                } else {
                    source
                        .get("data")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string()
                };

                return Some(WebFetchContent {
                    url,
                    content: content_data,
                    media_type,
                    title,
                    retrieved_at,
                });
            }
        }

        None
    }

    /// Parse citations from a text delta
    fn parse_citations(&self, citations_arr: &[Value]) -> Vec<Citation> {
        citations_arr
            .iter()
            .filter_map(|c| {
                // Handle web_search_result_location type
                let url = c
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                let title = c
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let cited_text = c
                    .get("cited_text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();

                if url.is_empty() && title.is_empty() {
                    None
                } else {
                    Some(Citation {
                        url,
                        title,
                        cited_text,
                    })
                }
            })
            .collect()
    }
}
