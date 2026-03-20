//! Anthropic Claude API provider implementation.

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AiError;
use crate::provider::AiProvider;
use crate::types::{AiContext, ErrorAnalysis, ErrorContext, Message, Role};

/// Default Anthropic API base URL.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// Default model for completions and chat.
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Timeout for regular (non-streaming) requests.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout for availability checks.
const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(5);
/// Channel capacity for streaming chunks.
const STREAM_CHANNEL_CAPACITY: usize = 64;

// ─── Internal API types ───────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize, Debug)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    /// The reason the model stopped generating tokens (e.g. `"end_turn"`).
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct ContentBlock {
    #[serde(rename = "type")]
    field_type: String,
    text: Option<String>,
}

// ─── Provider struct ──────────────────────────────────────────────────────────

/// Anthropic Claude AI provider.
///
/// Uses the [Messages API](https://docs.anthropic.com/en/api/messages) for
/// completions, streaming chat, and error analysis.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// # Arguments
    ///
    /// * `api_key`  – Anthropic API key (required).
    /// * `base_url` – Override the default API base URL.
    /// * `model`    – Override the default model.
    ///
    /// # Errors
    ///
    /// Returns [`AiError::Http`] if the underlying HTTP client cannot be built.
    pub fn new(
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Self, AiError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(AiError::Http)?;

        Ok(Self {
            client,
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        })
    }

    // ── HTTP helpers ──────────────────────────────────────────────────────────

    /// Attach the required Anthropic authentication headers to a request builder.
    fn auth_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
    }

    /// Map an HTTP status code to the appropriate [`AiError`].
    fn map_http_error(status: reqwest::StatusCode, body: &str) -> AiError {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return AiError::AuthenticationFailed(format!("HTTP 401: {body}"));
        }
        AiError::Provider(format!("HTTP {status}: {body}"))
    }

    /// Extract `retry-after` seconds from a response header.
    fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
        headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
    }

    /// Send a non-streaming Messages request and return the response body.
    async fn post_messages(&self, req: &MessagesRequest) -> Result<MessagesResponse, AiError> {
        let url = format!("{}/v1/messages", self.base_url);
        let builder = self.client.post(&url);
        let response = self
            .auth_headers(builder)
            .json(req)
            .send()
            .await
            .map_err(AiError::Http)?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = Self::parse_retry_after(response.headers());
            return Err(AiError::RateLimited { retry_after });
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body));
        }

        let body: MessagesResponse = response.json().await.map_err(AiError::Http)?;
        Ok(body)
    }
}

// ─── SSE parsing ──────────────────────────────────────────────────────────────

/// Parse a single SSE field line, returning the field name and its value.
///
/// Recognises `"event: <value>"` and `"data: <value>"` lines.
/// Returns `None` for comment lines, blank lines, and unrecognised fields.
fn parse_sse_line(line: &str) -> Option<(&str, &str)> {
    if let Some(value) = line.strip_prefix("event: ") {
        Some(("event", value.trim()))
    } else if let Some(value) = line.strip_prefix("data: ") {
        Some(("data", value.trim()))
    } else {
        None
    }
}

/// Map a [`Role`] to the lowercase string the Anthropic Messages API expects.
fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
    }
}

// ─── AiProvider impl ──────────────────────────────────────────────────────────

#[async_trait]
impl AiProvider for AnthropicProvider {
    async fn complete(&self, context: &AiContext) -> Result<String, AiError> {
        tracing::debug!(model = %self.model, "Sending completion request to Anthropic");
        let prompt = context.format_completion_prompt();

        let req = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 256,
            messages: vec![ApiMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            system: Some(
                "You are a terminal command completion assistant. \
                 Respond with only the completion text — no prose, no markdown."
                    .to_string(),
            ),
            stream: false,
        };

        let response = self.post_messages(&req).await?;

        let text = response
            .content
            .into_iter()
            .find(|b| b.field_type == "text")
            .and_then(|b| b.text)
            .unwrap_or_default();

        let completion = text.trim().to_string();

        // Strip the input prefix if the model echoed it back.
        let result = if let Some(stripped) = completion.strip_prefix(&context.input_prefix) {
            stripped.to_string()
        } else {
            completion
        };

        tracing::debug!(model = %self.model, "completion returned {} chars", result.len());
        Ok(result)
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        context: &AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError> {
        // Separate system prompt from conversation history.
        let system_prompt = messages
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| m.content.clone());

        // Build the API message list (exclude system messages — they go in `system`).
        let mut api_messages: Vec<ApiMessage> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| ApiMessage {
                role: role_to_str(m.role).to_string(),
                content: m.content.clone(),
            })
            .collect();

        // If the caller provided no non-system messages, seed with a context turn.
        if api_messages.is_empty() {
            api_messages.push(ApiMessage {
                role: "user".to_string(),
                content: context.format_completion_prompt(),
            });
        }

        let req = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 2048,
            messages: api_messages,
            system: system_prompt,
            stream: true,
        };

        let url = format!("{}/v1/messages", self.base_url);
        let builder = self.client.post(&url);
        let response = self
            .auth_headers(builder)
            .json(&req)
            .send()
            .await
            .map_err(AiError::Http)?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = Self::parse_retry_after(response.headers());
            return Err(AiError::RateLimited { retry_after });
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AiError::AuthenticationFailed(
                "HTTP 401 from streaming endpoint".to_string(),
            ));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(status = %status, body = %body, "Anthropic chat_stream returned non-success status");
            return Err(AiError::Provider(format!("HTTP {status}: {body}")));
        }

        // Spawn a task that reads the SSE body and forwards text deltas through a channel.
        let (tx, rx) = mpsc::channel::<Result<String, AiError>>(STREAM_CHANNEL_CAPACITY);

        tokio::spawn(async move {
            use tokio_stream::StreamExt as _;

            let byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_event: Option<String> = None;

            // Pin the stream and drive it with tokio_stream's StreamExt.
            let mut byte_stream = std::pin::pin!(byte_stream);

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Err(AiError::Http(e))).await;
                        return;
                    }
                };

                let text = match std::str::from_utf8(&chunk) {
                    Ok(s) => s.to_string(),
                    Err(e) => {
                        let _ = tx
                            .send(Err(AiError::StreamError(format!(
                                "UTF-8 decode error: {e}"
                            ))))
                            .await;
                        return;
                    }
                };

                buffer.push_str(&text);

                // Process complete lines (split on '\n').
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        // Empty line = end of SSE event; reset state.
                        current_event = None;
                        continue;
                    }

                    if let Some((field, value)) = parse_sse_line(&line) {
                        match field {
                            "event" => {
                                current_event = Some(value.to_string());
                                if value == "message_stop" {
                                    return;
                                }
                            }
                            "data" => {
                                if current_event.as_deref() == Some("content_block_delta") {
                                    // Extract delta.text from the JSON payload.
                                    if let Ok(json) =
                                        serde_json::from_str::<serde_json::Value>(value)
                                    {
                                        if let Some(delta_text) = json["delta"]["text"].as_str() {
                                            if !delta_text.is_empty()
                                                && tx
                                                    .send(Ok(delta_text.to_string()))
                                                    .await
                                                    .is_err()
                                            {
                                                // Receiver dropped.
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn analyze_error(&self, error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
        tracing::debug!(model = %self.model, command = %error.command, "Sending error analysis request to Anthropic");
        let user_prompt = error.format_error_analysis_prompt();

        let req = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            messages: vec![ApiMessage {
                role: "user".to_string(),
                content: user_prompt,
            }],
            system: Some(
                "You are an expert terminal error analyst. When asked to analyze an error, \
                 respond with only a JSON object — no markdown fences, no prose."
                    .to_string(),
            ),
            stream: false,
        };

        let response = self.post_messages(&req).await?;

        let raw_text = response
            .content
            .into_iter()
            .find(|b| b.field_type == "text")
            .and_then(|b| b.text)
            .unwrap_or_default();

        // Attempt to parse structured JSON; fall back to using the text as explanation.
        #[derive(serde::Deserialize)]
        struct AnalysisJson {
            explanation: String,
            #[serde(default)]
            suggestions: Vec<String>,
            #[serde(default)]
            confidence: f32,
        }

        let trimmed = raw_text.trim();

        // Strip optional ```json … ``` fences the model may emit.
        let json_str = trimmed
            .strip_prefix("```json")
            .and_then(|s| s.strip_suffix("```"))
            .map(str::trim)
            .unwrap_or(trimmed);

        if let Ok(parsed) = serde_json::from_str::<AnalysisJson>(json_str) {
            tracing::debug!(
                command = %error.command,
                confidence = parsed.confidence,
                "error analysis parsed successfully"
            );
            Ok(ErrorAnalysis {
                explanation: parsed.explanation,
                suggestions: parsed.suggestions,
                confidence: parsed.confidence,
            })
        } else {
            tracing::warn!(
                command = %error.command,
                "could not parse structured error analysis; using raw text"
            );
            Ok(ErrorAnalysis {
                explanation: trimmed.to_string(),
                suggestions: vec![],
                confidence: 0.5,
            })
        }
    }

    async fn is_available(&self) -> bool {
        self.client
            .head(&self.base_url)
            .timeout(AVAILABILITY_TIMEOUT)
            .send()
            .await
            .is_ok()
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ──────────────────────────────────────────────────────────

    #[test]
    fn test_construction_defaults() {
        let provider = AnthropicProvider::new("sk-ant-test".to_string(), None, None).unwrap();
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.model, DEFAULT_MODEL);
        assert_eq!(provider.api_key, "sk-ant-test");
    }

    #[test]
    fn test_construction_custom() {
        let provider = AnthropicProvider::new(
            "sk-ant-custom".to_string(),
            Some("https://proxy.example.com".to_string()),
            Some("claude-3-haiku-20240307".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "https://proxy.example.com");
        assert_eq!(provider.model, "claude-3-haiku-20240307");
    }

    // ── Prompt formatting ─────────────────────────────────────────────────────

    #[test]
    fn test_format_prompt() {
        let context = AiContext {
            cwd: Some("/home/user/project".to_string()),
            input_prefix: "git sta".to_string(),
            recent_output: vec!["$ ls".to_string(), "README.md".to_string()],
            shell: Some("zsh".to_string()),
            os: Some("macOS".to_string()),
            git_branch: Some("main".to_string()),
            ..Default::default()
        };
        let prompt = context.format_completion_prompt();
        assert!(prompt.contains("git sta"), "should include input prefix");
        assert!(prompt.contains("/home/user/project"), "should include cwd");
        assert!(prompt.contains("README.md"), "should include recent output");
        assert!(prompt.contains("zsh"), "should include shell");
        assert!(prompt.contains("main"), "should include git branch");
    }

    #[test]
    fn test_format_prompt_missing_optionals() {
        let context = AiContext {
            cwd: None,
            input_prefix: "ls".to_string(),
            recent_output: vec![],
            shell: None,
            os: None,
            git_branch: None,
            ..Default::default()
        };
        let prompt = context.format_completion_prompt();
        assert!(prompt.contains("unknown"), "should use 'unknown' fallback");
        assert!(
            prompt.contains("(none)"),
            "should use '(none)' for empty output"
        );
    }

    // ── SSE parsing ───────────────────────────────────────────────────────────

    #[test]
    fn test_sse_content_block_delta_parsing() {
        let sse_text = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\
             \"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n",
            "\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n",
            "\n",
        );

        let mut current_event: Option<&str> = None;
        let mut collected: Vec<String> = vec![];

        for line in sse_text.lines() {
            if line.is_empty() {
                current_event = None;
                continue;
            }
            if let Some((field, value)) = parse_sse_line(line) {
                match field {
                    "event" => current_event = Some(value),
                    "data" => {
                        if current_event == Some("content_block_delta") {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(value) {
                                if let Some(t) = json["delta"]["text"].as_str() {
                                    collected.push(t.to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        assert_eq!(collected, vec!["Hello"]);
    }

    // ── Response deserialization ───────────────────────────────────────────────

    #[test]
    fn test_response_deserialization() {
        let json = r#"{
            "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "Hello, world!"
                }
            ],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 25, "output_tokens": 4}
        }"#;

        let response: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.content[0].field_type, "text");
        assert_eq!(response.content[0].text.as_deref(), Some("Hello, world!"));
        assert_eq!(response.stop_reason.as_deref(), Some("end_turn"));
    }

    // ── Error status mapping ──────────────────────────────────────────────────

    #[test]
    fn test_error_status_mapping_401() {
        let status = reqwest::StatusCode::UNAUTHORIZED;
        let err = AnthropicProvider::map_http_error(status, "invalid api key");
        assert!(
            matches!(err, AiError::AuthenticationFailed(_)),
            "401 should map to AuthenticationFailed, got: {err:?}"
        );
    }

    #[test]
    fn test_error_status_mapping_500() {
        let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        let err = AnthropicProvider::map_http_error(status, "server error");
        assert!(
            matches!(err, AiError::Provider(_)),
            "500 should map to Provider error, got: {err:?}"
        );
    }

    #[test]
    fn test_name() {
        let provider = AnthropicProvider::new("sk-ant-test".to_string(), None, None).unwrap();
        assert_eq!(provider.name(), "anthropic");
    }
}
