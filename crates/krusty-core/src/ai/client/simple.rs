//! Simple (non-streaming) API calls
//!
//! Used for quick tasks like title generation where streaming is overkill.

use anyhow::Result;
use serde_json::Value;
use tracing::debug;

use super::core::AiClient;

impl AiClient {
    /// Make a simple non-streaming API call
    ///
    /// Used for quick tasks like title generation where streaming is overkill.
    /// Returns the text content directly. Routes to appropriate format handler.
    pub async fn call_simple(
        &self,
        model: &str,
        system_prompt: &str,
        user_message: &str,
        max_tokens: usize,
    ) -> Result<String> {
        // Route to appropriate format handler based on API format
        if self.config().uses_openai_format() {
            return self
                .call_simple_openai(model, system_prompt, user_message, max_tokens)
                .await;
        }

        if self.config().uses_google_format() {
            return self
                .call_simple_google(model, system_prompt, user_message, max_tokens)
                .await;
        }

        // Anthropic format (default)
        self.call_simple_anthropic(model, system_prompt, user_message, max_tokens)
            .await
    }

    /// Simple non-streaming call using Anthropic format
    async fn call_simple_anthropic(
        &self,
        model: &str,
        system_prompt: &str,
        user_message: &str,
        max_tokens: usize,
    ) -> Result<String> {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [{
                "role": "user",
                "content": user_message
            }],
            "system": system_prompt
        });

        let request = self.build_request(&self.config().api_url());
        let response = request.json(&body).send().await?;
        let response = self.handle_error_response(response).await?;

        let json: Value = response.json().await?;

        // Extract text from Anthropic response
        let text = json
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|block| block.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(text)
    }

    /// Simple non-streaming call using OpenAI format
    async fn call_simple_openai(
        &self,
        model: &str,
        system_prompt: &str,
        user_message: &str,
        max_tokens: usize,
    ) -> Result<String> {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_message}
            ]
        });

        let request = self.build_request(&self.config().api_url());
        let response = request.json(&body).send().await?;
        let response = self.handle_error_response(response).await?;

        let json: Value = response.json().await?;

        // Extract text from OpenAI response format
        let text = json
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(text)
    }

    /// Simple non-streaming call using Google format
    async fn call_simple_google(
        &self,
        model: &str,
        system_prompt: &str,
        user_message: &str,
        max_tokens: usize,
    ) -> Result<String> {
        let body = serde_json::json!({
            "contents": [{
                "role": "user",
                "parts": [{"text": user_message}]
            }],
            "systemInstruction": {
                "parts": [{"text": system_prompt}]
            },
            "generationConfig": {
                "maxOutputTokens": max_tokens
            }
        });

        let request = self.build_request(&self.config().api_url());
        debug!("Google simple call to model: {}", model);

        let response = request.json(&body).send().await?;
        let response = self.handle_error_response(response).await?;

        let json: Value = response.json().await?;

        // Extract text from Google response format
        let text = json
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array())
            .and_then(|arr| arr.first())
            .and_then(|part| part.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(text)
    }
}
