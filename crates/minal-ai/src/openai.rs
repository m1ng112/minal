//! OpenAI API provider implementation.

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AiError;
use crate::provider::AiProvider;
use crate::types::{AiContext, ErrorAnalysis, ErrorContext, Message, Role};

/// Default OpenAI API base URL.
const DEFAULT_BASE_URL: &str = "https://api.openai.com";
/// Default model for completions.
const DEFAULT_MODEL: &str = "gpt-4o";
/// Timeout for completion requests (in seconds).
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout for availability checks.
const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(5);
/// SSE stream channel buffer size.
const STREAM_CHANNEL_BUFFER: usize = 64;

// ── Internal request/response types ─────────────────────────────────────────

#[derive(serde::Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(serde::Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    /// Reason the model stopped generating (e.g., `"stop"`, `"length"`).
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

/// A single chunk received from the streaming SSE endpoint.
#[derive(serde::Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(serde::Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    /// Reason the stream ended; `None` for mid-stream chunks.
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────────────────────

/// OpenAI-compatible AI provider (supports OpenAI and any OpenAI-compatible endpoint).
pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiProvider {
    /// Creates a new OpenAI provider.
    ///
    /// # Arguments
    /// - `api_key` – Your OpenAI (or compatible) API key.
    /// - `base_url` – Override the base URL (default: `https://api.openai.com`).
    /// - `model` – Override the model (default: `gpt-4o`).
    ///
    /// # Errors
    /// Returns [`AiError::Http`] if the underlying HTTP client cannot be built.
    pub fn new(
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Result<Self, AiError> {
        let client = reqwest::Client::builder()
            .timeout(COMPLETION_TIMEOUT)
            .build()
            .map_err(AiError::Http)?;

        Ok(Self {
            client,
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        })
    }

    /// Maps an HTTP status code to the appropriate [`AiError`] variant.
    fn map_status_error(status: reqwest::StatusCode, body: &str) -> AiError {
        match status.as_u16() {
            401 | 403 => AiError::AuthenticationFailed(format!("status {status}: {body}")),
            429 => AiError::RateLimited { retry_after: None },
            _ => AiError::Provider(format!("OpenAI returned status {status}: {body}")),
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    async fn complete(&self, context: &AiContext) -> Result<String, AiError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: context.format_completion_prompt(),
            }],
            stream: false,
            max_tokens: Some(256),
        };

        tracing::debug!(model = %self.model, "sending completion request to OpenAI");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "(unreadable body)".to_string());
            return Err(Self::map_status_error(status, &body));
        }

        let body: ChatCompletionResponse = response.json().await.map_err(AiError::Http)?;

        let content = body
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content.trim().to_string())
            .unwrap_or_default();

        tracing::debug!(len = content.len(), "received completion from OpenAI");
        Ok(content)
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        context: &AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        // Build messages list: convert public Message type to wire ChatMessage.
        let mut chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(|m| ChatMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                }
                .to_string(),
                content: m.content.clone(),
            })
            .collect();

        // Append current context as a user message if there is an active input.
        if !context.input_prefix.is_empty() {
            chat_messages.push(ChatMessage {
                role: "user".to_string(),
                content: context.input_prefix.clone(),
            });
        }

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: chat_messages,
            stream: true,
            max_tokens: None,
        };

        tracing::debug!(model = %self.model, "starting chat stream with OpenAI");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "(unreadable body)".to_string());
            return Err(Self::map_status_error(status, &body));
        }

        let (tx, rx) = mpsc::channel::<Result<String, AiError>>(STREAM_CHANNEL_BUFFER);

        // Spawn a task that reads SSE lines and forwards token chunks.
        tokio::spawn(async move {
            use tokio_stream::StreamExt as _;

            // Collect response bytes line by line using the raw byte stream.
            let mut byte_stream = Box::pin(response.bytes_stream());
            let mut buffer = String::new();

            'outer: loop {
                match byte_stream.next().await {
                    None => break,
                    Some(Err(e)) => {
                        let _ = tx.send(Err(AiError::StreamError(e.to_string()))).await;
                        break;
                    }
                    Some(Ok(raw_chunk)) => {
                        let text = match std::str::from_utf8(&raw_chunk) {
                            Ok(t) => t,
                            Err(e) => {
                                tracing::warn!(error = %e, "invalid UTF-8 in SSE stream");
                                continue;
                            }
                        };
                        buffer.push_str(text);

                        // Process all complete lines in the buffer.
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer.drain(..=newline_pos);

                            if line.is_empty() {
                                continue;
                            }

                            let data = match line.strip_prefix("data: ") {
                                Some(d) => d.trim().to_string(),
                                None => continue, // non-data SSE field, skip
                            };

                            if data == "[DONE]" {
                                break 'outer;
                            }

                            match serde_json::from_str::<StreamChunk>(&data) {
                                Ok(chunk) => {
                                    for choice in &chunk.choices {
                                        if let Some(text) = &choice.delta.content {
                                            if !text.is_empty()
                                                && tx.send(Ok(text.clone())).await.is_err()
                                            {
                                                // Receiver dropped – stop streaming.
                                                break 'outer;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, raw = %data, "failed to parse SSE chunk");
                                }
                            }
                        }
                    }
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn analyze_error(&self, error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "You are a helpful terminal assistant that analyzes command errors \
                              and suggests fixes. Always respond with valid JSON."
                        .to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: error.format_error_analysis_prompt(),
                },
            ],
            stream: false,
            max_tokens: Some(512),
        };

        tracing::debug!(command = %error.command, "sending error analysis request to OpenAI");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "(unreadable body)".to_string());
            return Err(Self::map_status_error(status, &body));
        }

        let completion: ChatCompletionResponse = response.json().await.map_err(AiError::Http)?;

        let raw = completion
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default();

        parse_error_analysis(&raw)
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/v1/models", self.base_url);
        self.client
            .get(&url)
            .bearer_auth(&self.api_key)
            .timeout(AVAILABILITY_TIMEOUT)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }

    fn name(&self) -> &str {
        "openai"
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parses the raw JSON string returned by the model into an [`ErrorAnalysis`].
///
/// Accepts the full JSON object, optionally wrapped in markdown code fences.
fn parse_error_analysis(raw: &str) -> Result<ErrorAnalysis, AiError> {
    // Strip common markdown code fences that models sometimes add.
    let trimmed = raw.trim();
    let json_str = if let Some(inner) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        inner.trim_end_matches("```").trim()
    } else {
        trimmed
    };

    #[derive(serde::Deserialize)]
    struct RawAnalysis {
        explanation: String,
        suggestions: Vec<String>,
        confidence: f32,
    }

    let parsed: RawAnalysis = serde_json::from_str(json_str).map_err(|e| {
        AiError::Provider(format!(
            "failed to parse error analysis JSON: {e}. Raw response: {raw}"
        ))
    })?;

    Ok(ErrorAnalysis {
        explanation: parsed.explanation,
        suggestions: parsed.suggestions,
        confidence: parsed.confidence.clamp(0.0, 1.0),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construction_defaults() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None, None).unwrap();
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.model, DEFAULT_MODEL);
        assert_eq!(provider.api_key, "sk-test");
    }

    #[test]
    fn test_construction_custom() {
        let provider = OpenAiProvider::new(
            "sk-custom".to_string(),
            Some("https://my.openai.proxy.com".to_string()),
            Some("gpt-3.5-turbo".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "https://my.openai.proxy.com");
        assert_eq!(provider.model, "gpt-3.5-turbo");
        assert_eq!(provider.api_key, "sk-custom");
    }

    #[test]
    fn test_format_completion_prompt() {
        let context = AiContext {
            cwd: Some("/home/user/project".to_string()),
            input_prefix: "git sta".to_string(),
            recent_output: vec!["$ ls".to_string(), "src/  Cargo.toml".to_string()],
            shell: Some("zsh".to_string()),
            os: Some("macOS".to_string()),
            git_branch: Some("main".to_string()),
            ..Default::default()
        };
        let prompt = context.format_completion_prompt();
        assert!(prompt.contains("git sta"));
        assert!(prompt.contains("/home/user/project"));
        assert!(prompt.contains("src/  Cargo.toml"));
        assert!(prompt.contains("zsh"));
        assert!(prompt.contains("macOS"));
        assert!(prompt.contains("main"));
    }

    #[test]
    fn test_format_completion_prompt_defaults() {
        let context = AiContext {
            cwd: None,
            input_prefix: "ls".to_string(),
            recent_output: vec![],
            ..Default::default()
        };
        let prompt = context.format_completion_prompt();
        assert!(prompt.contains("unknown"));
        assert!(prompt.contains("(none)"));
        assert!(prompt.contains("ls"));
    }

    #[test]
    fn test_sse_parsing_done_signal() {
        // Ensure "[DONE]" is recognised as end-of-stream.
        // The actual closure returns false when it sees this sentinel; here we
        // verify the string parsing step that feeds into that decision.
        let line = "data: [DONE]";
        let data = line.strip_prefix("data: ").unwrap().trim();
        assert_eq!(data, "[DONE]");
    }

    #[test]
    fn test_sse_parsing_content_chunk() {
        let json = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":1,"model":"gpt-4o","choices":[{"delta":{"content":"hello"},"finish_reason":null,"index":0}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_sse_parsing_finish_reason() {
        let json = r#"{"id":"chatcmpl-2","object":"chat.completion.chunk","created":1,"model":"gpt-4o","choices":[{"delta":{},"finish_reason":"stop","index":0}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        assert!(chunk.choices[0].delta.content.is_none());
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "created": 1700000000,
            "model": "gpt-4o",
            "choices": [
                {
                    "message": {"role": "assistant", "content": "git status"},
                    "finish_reason": "stop",
                    "index": 0
                }
            ]
        }"#;
        let resp: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, "git status");
        assert_eq!(resp.choices[0].finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_error_status_mapping_401() {
        let err =
            OpenAiProvider::map_status_error(reqwest::StatusCode::UNAUTHORIZED, "invalid key");
        assert!(matches!(err, AiError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_error_status_mapping_403() {
        let err = OpenAiProvider::map_status_error(reqwest::StatusCode::FORBIDDEN, "forbidden");
        assert!(matches!(err, AiError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_error_status_mapping_429() {
        let err =
            OpenAiProvider::map_status_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "slow down");
        assert!(matches!(err, AiError::RateLimited { retry_after: None }));
    }

    #[test]
    fn test_error_status_mapping_500() {
        let err = OpenAiProvider::map_status_error(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "server error",
        );
        assert!(matches!(err, AiError::Provider(_)));
    }

    #[test]
    fn test_parse_error_analysis_valid_json() {
        let raw = r#"{"explanation":"File not found","suggestions":["Check the path","Use ls to list files"],"confidence":0.9}"#;
        let analysis = parse_error_analysis(raw).unwrap();
        assert_eq!(analysis.explanation, "File not found");
        assert_eq!(analysis.suggestions.len(), 2);
        assert!((analysis.confidence - 0.9).abs() < 1e-5);
    }

    #[test]
    fn test_parse_error_analysis_markdown_fenced() {
        let raw = "```json\n{\"explanation\":\"Permission denied\",\"suggestions\":[\"Use sudo\"],\"confidence\":0.8}\n```";
        let analysis = parse_error_analysis(raw).unwrap();
        assert_eq!(analysis.explanation, "Permission denied");
        assert!((analysis.confidence - 0.8).abs() < 1e-5);
    }

    #[test]
    fn test_parse_error_analysis_confidence_clamped() {
        let raw = r#"{"explanation":"test","suggestions":["fix"],"confidence":1.5}"#;
        let analysis = parse_error_analysis(raw).unwrap();
        assert!((analysis.confidence - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_parse_error_analysis_invalid_json() {
        let raw = "not json at all";
        let err = parse_error_analysis(raw);
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), AiError::Provider(_)));
    }

    #[test]
    fn test_name() {
        let provider = OpenAiProvider::new("sk-test".to_string(), None, None).unwrap();
        assert_eq!(provider.name(), "openai");
    }
}
