//! OpenAI-compatible SSE parser for chat/completions format

use anyhow::Result;
use serde_json::Value;

use crate::ai::sse::{SseEvent, SseParser, ToolCallAccumulator};
use crate::ai::types::{FinishReason, Usage};

/// OpenAI-compatible SSE parser for chat/completions format
pub struct OpenAIParser {
    /// Track tool calls being accumulated
    tool_accumulators: std::sync::Mutex<std::collections::HashMap<usize, ToolCallAccumulator>>,
}

impl OpenAIParser {
    pub fn new() -> Self {
        Self {
            tool_accumulators: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Lock tool accumulators with proper error handling
    fn lock_tool_accumulators(
        &self,
    ) -> anyhow::Result<
        std::sync::MutexGuard<'_, std::collections::HashMap<usize, ToolCallAccumulator>>,
    > {
        self.tool_accumulators
            .lock()
            .map_err(|e| anyhow::anyhow!("Tool accumulators lock poisoned: {}", e))
    }

    /// Parse OpenAI Responses API event format
    /// Used by GPT-5 models via OpenCode Zen and ChatGPT Codex
    fn parse_responses_api_event(&self, json: &Value, event_type: &str) -> SseEvent {
        tracing::debug!("Responses API event: {} - {:?}", event_type, json);

        match event_type {
            // Text content delta
            "response.output_text.delta" => {
                if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                    if !delta.is_empty() {
                        return SseEvent::TextDelta(delta.to_string());
                    }
                }
            }

            // Reasoning/thinking deltas (OpenAI o1, codex, GPT-5 reasoning models)
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                    if !delta.is_empty() {
                        tracing::debug!(
                            "Reasoning delta: {}...",
                            &delta.chars().take(50).collect::<String>()
                        );
                        return SseEvent::ThinkingDelta {
                            index: 0,
                            thinking: delta.to_string(),
                        };
                    }
                }
            }

            // Reasoning part started - emit ThinkingStart
            "response.reasoning_summary_part.added" => {
                tracing::info!("Reasoning block started");
                return SseEvent::ThinkingStart { index: 0 };
            }

            // Reasoning complete
            "response.reasoning_summary_text.done"
            | "response.reasoning_text.done"
            | "response.reasoning_summary_part.done" => {
                tracing::info!("Reasoning block complete");
                return SseEvent::ThinkingComplete {
                    index: 0,
                    thinking: String::new(), // Already sent via deltas
                    signature: String::new(),
                };
            }

            // Response completed - check for accumulated tool calls and extract usage
            "response.done" | "response.completed" => {
                // Extract usage from the response field (Codex API format)
                // Structure: {"type": "response.done", "response": {"id": "...", "usage": {...}}}
                let usage = json.get("response").and_then(|r| r.get("usage")).map(|u| {
                    let input =
                        u.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                    let output =
                        u.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                    let cached = u
                        .get("input_tokens_details")
                        .and_then(|d| d.get("cached_tokens"))
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0) as usize;
                    tracing::info!(
                        "Responses API usage: input={}, output={}, cached={}",
                        input,
                        output,
                        cached
                    );
                    Usage {
                        prompt_tokens: input,
                        completion_tokens: output,
                        total_tokens: input + output,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: cached,
                    }
                });

                if let Ok(mut accumulators) = self.lock_tool_accumulators() {
                    if !accumulators.is_empty() {
                        // Complete all accumulated tool calls
                        let tool_calls: Vec<_> = accumulators
                            .drain()
                            .map(|(_, mut acc)| acc.force_complete())
                            .collect();
                        tracing::info!(
                            "Responses API completing with {} tool calls",
                            tool_calls.len()
                        );
                        return SseEvent::FinishWithToolCalls { tool_calls, usage };
                    }
                }
                return SseEvent::Finish {
                    reason: FinishReason::Stop,
                    usage,
                };
            }

            // Function/tool call start - multiple event types for different APIs
            "response.function_call_arguments.start" | "response.output_item.added" => {
                if let Some(item) = json.get("item") {
                    if item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                        let id = item
                            .get("call_id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            tracing::info!(
                                "Responses API tool call start: id={}, name={}",
                                id,
                                name
                            );
                            if let Ok(mut accumulators) = self.lock_tool_accumulators() {
                                let index = accumulators.len();
                                accumulators.insert(
                                    index,
                                    ToolCallAccumulator::new(id.clone(), name.clone()),
                                );
                            }
                            return SseEvent::ToolCallStart { id, name };
                        }
                    }
                }
            }

            // Function arguments delta
            "response.function_call_arguments.delta" => {
                if let Some(delta) = json.get("delta").and_then(|d| d.as_str()) {
                    if let Ok(mut accumulators) = self.lock_tool_accumulators() {
                        if let Some((_, acc)) = accumulators.iter_mut().last() {
                            acc.add_arguments(delta);
                            return SseEvent::ToolCallDelta {
                                id: acc.id.clone(),
                                delta: delta.to_string(),
                            };
                        }
                    }
                }
            }

            // Function call done - arguments are complete
            "response.function_call_arguments.done" => {
                // Arguments complete, tool call will be finalized on response.done
                if let Some(arguments) = json.get("arguments").and_then(|a| a.as_str()) {
                    if let Ok(mut accumulators) = self.lock_tool_accumulators() {
                        if let Some((_, acc)) = accumulators.iter_mut().last() {
                            // Ensure we have the complete arguments
                            if acc.arguments.is_empty() {
                                acc.add_arguments(arguments);
                            }
                            tracing::debug!(
                                "Tool call arguments complete: id={}, args_len={}",
                                acc.id,
                                acc.arguments.len()
                            );
                        }
                    }
                }
            }

            // Usage info - handle both standard Responses API and Codex field names
            "response.usage" => {
                // Try nested "usage" object first, then top-level fields
                let usage_obj = json.get("usage").unwrap_or(json);

                // Try both naming conventions:
                // Standard: input_tokens, output_tokens
                // Codex: input, output (and total, cached_input, reasoning_output)
                let prompt = usage_obj
                    .get("input_tokens")
                    .or_else(|| usage_obj.get("input"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;

                let completion = usage_obj
                    .get("output_tokens")
                    .or_else(|| usage_obj.get("output"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;

                let cached = usage_obj
                    .get("cached_input")
                    .or_else(|| usage_obj.get("cache_read_input_tokens"))
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;

                if prompt > 0 || completion > 0 {
                    tracing::info!(
                        "Responses API usage: input={}, output={}, cached={}",
                        prompt,
                        completion,
                        cached
                    );
                    return SseEvent::Usage(Usage {
                        prompt_tokens: prompt,
                        completion_tokens: completion,
                        total_tokens: prompt + completion,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: cached,
                    });
                }
            }

            // Other events we can skip
            _ => {
                tracing::trace!("Skipping Responses API event: {}", event_type);
            }
        }

        SseEvent::Skip
    }
}

impl Default for OpenAIParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SseParser for OpenAIParser {
    async fn parse_event(&self, json: &Value) -> Result<SseEvent> {
        // Check for error response first
        // OpenAI format: {"error": {"message": "...", "type": "...", "code": "..."}}
        if let Some(error) = json.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            let error_type = error
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            return Err(anyhow::anyhow!(
                "OpenAI API error ({}): {}",
                error_type,
                message
            ));
        }

        // Check for Responses API format (has "type" field)
        if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
            // Check for error events in Responses API
            if event_type == "error" || event_type.contains("error") {
                let message = json
                    .get("message")
                    .and_then(|m| m.as_str())
                    .or_else(|| json.get("error").and_then(|e| e.as_str()))
                    .unwrap_or("Unknown error");
                return Err(anyhow::anyhow!("OpenAI Responses API error: {}", message));
            }
            return Ok(self.parse_responses_api_event(json, event_type));
        }

        // OpenAI Chat Completions format: {"choices": [{"index": 0, "delta": {...}, "finish_reason": null}]}
        let choices = json.get("choices").and_then(|c| c.as_array());

        if let Some(choices) = choices {
            if let Some(choice) = choices.first() {
                // Check for finish_reason
                if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
                    if reason == "stop" || reason == "end_turn" {
                        return Ok(SseEvent::Finish {
                            reason: FinishReason::Stop,
                            usage: None,
                        });
                    }
                    if reason == "tool_calls" {
                        // Complete all accumulated tool calls
                        let mut accumulators = self.lock_tool_accumulators()?;
                        let tool_calls: Vec<_> = accumulators
                            .drain()
                            .map(|(_, mut acc)| acc.force_complete())
                            .collect();

                        if tool_calls.is_empty() {
                            return Ok(SseEvent::Finish {
                                reason: FinishReason::ToolCalls,
                                usage: None,
                            });
                        }
                        return Ok(SseEvent::FinishWithToolCalls {
                            tool_calls,
                            usage: None,
                        });
                    }
                }

                // Check for delta content
                if let Some(delta) = choice.get("delta") {
                    // Regular text content
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            return Ok(SseEvent::TextDelta(content.to_string()));
                        }
                    }

                    // Reasoning content (GLM-style thinking)
                    if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str())
                    {
                        if !reasoning.is_empty() {
                            // Treat reasoning as thinking delta
                            return Ok(SseEvent::ThinkingDelta {
                                index: 0,
                                thinking: reasoning.to_string(),
                            });
                        }
                    }

                    // Tool calls
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tool_call in tool_calls {
                            let index = tool_call.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                as usize;

                            // Check for function info (start of tool call)
                            if let Some(function) = tool_call.get("function") {
                                let id = tool_call
                                    .get("id")
                                    .and_then(|i| i.as_str())
                                    .unwrap_or("")
                                    .to_string();

                                if let Some(name) = function.get("name").and_then(|n| n.as_str()) {
                                    // New tool call starting
                                    let mut accumulators = self.lock_tool_accumulators()?;
                                    accumulators.insert(
                                        index,
                                        ToolCallAccumulator::new(id.clone(), name.to_string()),
                                    );
                                    return Ok(SseEvent::ToolCallStart {
                                        id,
                                        name: name.to_string(),
                                    });
                                }

                                if let Some(args) =
                                    function.get("arguments").and_then(|a| a.as_str())
                                {
                                    // Arguments delta
                                    let mut accumulators = self.lock_tool_accumulators()?;
                                    if let Some(acc) = accumulators.get_mut(&index) {
                                        acc.add_arguments(args);
                                        return Ok(SseEvent::ToolCallDelta {
                                            id: acc.id.clone(),
                                            delta: args.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for usage info
        if let Some(usage) = json.get("usage") {
            let prompt_tokens = usage
                .get("prompt_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize;
            let completion_tokens = usage
                .get("completion_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize;
            if prompt_tokens > 0 || completion_tokens > 0 {
                return Ok(SseEvent::Usage(Usage {
                    prompt_tokens,
                    completion_tokens,
                    total_tokens: prompt_tokens + completion_tokens,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                }));
            }
        }

        // Check for [DONE] marker (OpenAI uses this)
        // This is handled at the SSE line level, but just in case
        Ok(SseEvent::Skip)
    }
}
