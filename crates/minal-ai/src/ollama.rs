//! Ollama AI provider implementation.

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::error::AiError;
use crate::provider::AiProvider;
use crate::types::{AiContext, ErrorAnalysis, ErrorContext, Message, Role};

/// Default Ollama API base URL.
const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Default model for completions.
const DEFAULT_MODEL: &str = "codellama:7b";
/// Availability check timeout.
const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(2);
/// Warmup request timeout (model loading can be slow).
const WARMUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Channel buffer size for streaming responses.
const STREAM_CHANNEL_CAPACITY: usize = 64;

/// Ollama-based AI completion provider.
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[derive(serde::Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(serde::Deserialize)]
struct GenerateResponse {
    response: String,
}

/// Request body for Ollama's `/api/chat` endpoint.
#[derive(serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

/// A single message for the Ollama chat API.
#[derive(serde::Serialize, serde::Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// A single line of the NDJSON stream from `/api/chat`.
#[derive(serde::Deserialize)]
struct ChatStreamResponse {
    message: Option<ChatMessage>,
    done: bool,
}

/// JSON shape expected from the model for error analysis.
#[derive(serde::Deserialize)]
struct AnalysisJson {
    explanation: String,
    #[serde(default)]
    suggestions: Vec<String>,
    #[serde(default = "default_confidence")]
    confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

impl OllamaProvider {
    /// Creates a new Ollama provider.
    ///
    /// Uses default base URL (`http://localhost:11434`) and model (`codellama:7b`)
    /// if not specified.
    ///
    /// # Errors
    /// Returns `AiError::Http` if the HTTP client cannot be constructed.
    pub fn new(base_url: Option<String>, model: Option<String>) -> Result<Self, AiError> {
        // No client-level timeout — timeout enforcement is delegated to
        // `FallbackProvider` which wraps each call with the user-configured
        // `completion_timeout_ms` value.
        let client = reqwest::Client::builder().build().map_err(AiError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        })
    }

    /// Converts a [`Message`] into the Ollama chat wire format.
    fn to_chat_message(msg: &Message) -> ChatMessage {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        ChatMessage {
            role: role.to_string(),
            content: msg.content.clone(),
        }
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    async fn complete(&self, context: &AiContext) -> Result<String, AiError> {
        tracing::debug!(model = %self.model, "Sending completion request to Ollama");
        let url = format!("{}/api/generate", self.base_url);
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: context.format_completion_prompt(),
            stream: false,
        };
        let input_prefix = context.input_prefix.clone();

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        if !response.status().is_success() {
            return Err(AiError::Provider(format!(
                "Ollama returned status {}",
                response.status()
            )));
        }

        let body: GenerateResponse = response.json().await.map_err(AiError::Http)?;
        let completion = body.response.trim().to_string();

        // Strip the input prefix if the model echoed it back.
        let result = if let Some(stripped) = completion.strip_prefix(&input_prefix) {
            stripped.to_string()
        } else {
            completion
        };

        Ok(result)
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        _context: &AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError> {
        let url = format!("{}/api/chat", self.base_url);
        let chat_messages: Vec<ChatMessage> = messages.iter().map(Self::to_chat_message).collect();

        let request = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages,
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AiError::AuthenticationFailed(
                "Ollama returned 401 Unauthorized".to_string(),
            ));
        }
        if !status.is_success() {
            return Err(AiError::Provider(format!(
                "Ollama chat returned status {status}"
            )));
        }

        let (tx, rx) = mpsc::channel::<Result<String, AiError>>(STREAM_CHANNEL_CAPACITY);

        // Spawn a task that reads NDJSON lines from the response body and forwards
        // each content chunk through the channel.
        tokio::spawn(async move {
            use bytes::Bytes;
            use tokio_stream::StreamExt as _;

            // Box and pin the byte stream so we can poll it without an `Unpin` bound.
            let mut byte_stream: Pin<
                Box<dyn futures_core::Stream<Item = Result<Bytes, reqwest::Error>> + Send>,
            > = Box::pin(response.bytes_stream());

            // Buffer for accumulating partial lines across chunk boundaries.
            let mut line_buf = Vec::<u8>::new();

            loop {
                match byte_stream.next().await {
                    None => break,
                    Some(Err(e)) => {
                        let _ = tx.send(Err(AiError::StreamError(e.to_string()))).await;
                        break;
                    }
                    Some(Ok(bytes)) => {
                        line_buf.extend_from_slice(&bytes);

                        // Process all complete newline-delimited lines.
                        while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = line_buf.drain(..=pos).collect();
                            let trimmed = line.trim_ascii();
                            if trimmed.is_empty() {
                                continue;
                            }

                            match serde_json::from_slice::<ChatStreamResponse>(trimmed) {
                                Err(e) => {
                                    let _ = tx
                                        .send(Err(AiError::StreamError(format!(
                                            "JSON parse error: {e}"
                                        ))))
                                        .await;
                                    return;
                                }
                                Ok(parsed) => {
                                    if parsed.done {
                                        return;
                                    }
                                    if let Some(msg) = parsed.message {
                                        if !msg.content.is_empty()
                                            && tx.send(Ok(msg.content)).await.is_err()
                                        {
                                            // Receiver dropped – stop streaming.
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn analyze_error(&self, error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
        let url = format!("{}/api/generate", self.base_url);
        let prompt = error.format_error_analysis_prompt();

        tracing::debug!(command = %error.command, "Requesting error analysis from Ollama");

        let request = GenerateRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        if !response.status().is_success() {
            return Err(AiError::Provider(format!(
                "Ollama returned status {} for error analysis",
                response.status()
            )));
        }

        let body: GenerateResponse = response.json().await.map_err(AiError::Http)?;
        let raw = body.response.trim().to_string();

        // Attempt to parse the model's output as structured JSON.
        match serde_json::from_str::<AnalysisJson>(&raw) {
            Ok(parsed) => Ok(ErrorAnalysis {
                explanation: parsed.explanation,
                suggestions: parsed.suggestions,
                confidence: parsed.confidence,
            }),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Ollama error analysis response was not valid JSON; using raw text"
                );
                Ok(ErrorAnalysis {
                    explanation: raw,
                    suggestions: vec![],
                    confidence: 0.5,
                })
            }
        }
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client
            .get(&url)
            .timeout(AVAILABILITY_TIMEOUT)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }

    fn name(&self) -> &str {
        "ollama"
    }

    async fn warmup(&self) -> Result<(), AiError> {
        tracing::info!(model = %self.model, "Warming up Ollama (loading model into memory)");
        let url = format!("{}/api/generate", self.base_url);
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: "hi".to_string(),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .timeout(WARMUP_TIMEOUT)
            .json(&request)
            .send()
            .await
            .map_err(AiError::Http)?;

        if !response.status().is_success() {
            return Err(AiError::Provider(format!(
                "Ollama warmup returned status {}",
                response.status()
            )));
        }

        // Consume the response body to ensure model is fully loaded.
        let _body: GenerateResponse = response.json().await.map_err(AiError::Http)?;
        tracing::info!(model = %self.model, "Ollama warmup complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_construction() {
        let provider = OllamaProvider::new(None, None).unwrap();
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.model, DEFAULT_MODEL);
    }

    #[test]
    fn test_custom_construction() {
        let provider = OllamaProvider::new(
            Some("http://example.com:11434".to_string()),
            Some("llama2:13b".to_string()),
        )
        .unwrap();
        assert_eq!(provider.base_url, "http://example.com:11434");
        assert_eq!(provider.model, "llama2:13b");
    }

    #[test]
    fn test_format_prompt() {
        let context = AiContext {
            cwd: Some("/home/user".to_string()),
            input_prefix: "git sta".to_string(),
            recent_output: vec!["$ ls".to_string(), "file.txt".to_string()],
            ..Default::default()
        };
        let prompt = context.format_completion_prompt();
        assert!(prompt.contains("git sta"));
        assert!(prompt.contains("/home/user"));
        assert!(prompt.contains("file.txt"));
    }

    #[test]
    fn test_format_prompt_no_cwd() {
        let context = AiContext {
            cwd: None,
            input_prefix: "ls".to_string(),
            recent_output: vec![],
            ..Default::default()
        };
        let prompt = context.format_completion_prompt();
        assert!(prompt.contains("unknown"));
        assert!(prompt.contains("(none)"));
    }

    #[test]
    fn test_name() {
        let provider = OllamaProvider::new(None, None).unwrap();
        assert_eq!(provider.name(), "ollama");
    }

    // --- New tests ---

    #[test]
    fn test_format_error_prompt() {
        let error = ErrorContext {
            command: "cargo build".to_string(),
            exit_code: 1,
            stderr: "error[E0308]: mismatched types".to_string(),
            stdout: String::new(),
            cwd: Some("/home/user/project".to_string()),
        };
        let prompt = error.format_error_analysis_prompt();
        assert!(prompt.contains("cargo build"));
        assert!(prompt.contains("Exit code: 1"));
        assert!(prompt.contains("/home/user/project"));
        assert!(prompt.contains("error[E0308]: mismatched types"));
        assert!(prompt.contains("JSON"));
        assert!(prompt.contains("explanation"));
        assert!(prompt.contains("suggestions"));
    }

    #[test]
    fn test_format_error_prompt_no_cwd() {
        let error = ErrorContext {
            command: "ls /nonexistent".to_string(),
            exit_code: 2,
            stderr: "No such file or directory".to_string(),
            stdout: String::new(),
            cwd: None,
        };
        let prompt = error.format_error_analysis_prompt();
        assert!(prompt.contains("ls /nonexistent"));
        assert!(prompt.contains("Exit code: 2"));
        assert!(prompt.contains("unknown"));
        assert!(prompt.contains("No such file or directory"));
    }

    #[test]
    fn test_chat_stream_response_deserialization() {
        // Non-final chunk with content.
        let chunk_json = r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let parsed: ChatStreamResponse = serde_json::from_str(chunk_json).unwrap();
        assert!(!parsed.done);
        let msg = parsed.message.unwrap();
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.role, "assistant");

        // Final chunk (done = true, no message content).
        let done_json = r#"{"done":true}"#;
        let parsed_done: ChatStreamResponse = serde_json::from_str(done_json).unwrap();
        assert!(parsed_done.done);
        assert!(parsed_done.message.is_none());

        // Chunk with empty content should be tolerated.
        let empty_json = r#"{"message":{"role":"assistant","content":""},"done":false}"#;
        let parsed_empty: ChatStreamResponse = serde_json::from_str(empty_json).unwrap();
        assert!(!parsed_empty.done);
        assert_eq!(parsed_empty.message.unwrap().content, "");
    }

    #[test]
    fn test_analysis_response_parsing() {
        // Well-formed JSON response.
        let json = r#"{"explanation":"Command not found","suggestions":["Install the tool","Check PATH"],"confidence":0.9}"#;
        let parsed: AnalysisJson = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.explanation, "Command not found");
        assert_eq!(parsed.suggestions.len(), 2);
        assert!((parsed.confidence - 0.9_f32).abs() < f32::EPSILON);

        // Verify construction of ErrorAnalysis from the parsed data.
        let analysis = ErrorAnalysis {
            explanation: parsed.explanation.clone(),
            suggestions: parsed.suggestions.clone(),
            confidence: parsed.confidence,
        };
        assert_eq!(analysis.explanation, "Command not found");
        assert_eq!(analysis.suggestions[0], "Install the tool");
        assert_eq!(analysis.suggestions[1], "Check PATH");

        // Fallback for non-JSON raw text.
        let raw = "The command was not found in PATH.";
        let fallback = ErrorAnalysis {
            explanation: raw.to_string(),
            suggestions: vec![],
            confidence: 0.5,
        };
        assert_eq!(fallback.explanation, raw);
        assert!(fallback.suggestions.is_empty());
        assert!((fallback.confidence - 0.5_f32).abs() < f32::EPSILON);
    }
}
