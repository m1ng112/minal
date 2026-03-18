//! Ollama AI provider implementation.

use std::pin::Pin;
use std::time::Duration;

use crate::AiError;
use crate::provider::{AiProvider, CompletionContext};

/// Default Ollama API base URL.
const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Default model for completions.
const DEFAULT_MODEL: &str = "codellama:7b";
/// Timeout for completion requests.
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);
/// Timeout for availability checks.
const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(2);

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

impl OllamaProvider {
    /// Creates a new Ollama provider.
    ///
    /// Uses default base URL (`http://localhost:11434`) and model (`codellama:7b`)
    /// if not specified.
    ///
    /// # Errors
    /// Returns `AiError::Http` if the HTTP client cannot be constructed.
    pub fn new(base_url: Option<String>, model: Option<String>) -> Result<Self, AiError> {
        let client = reqwest::Client::builder()
            .timeout(COMPLETION_TIMEOUT)
            .build()
            .map_err(AiError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        })
    }

    /// Formats the prompt for the Ollama API.
    fn format_prompt(context: &CompletionContext) -> String {
        let cwd = context.cwd.as_deref().unwrap_or("unknown");
        let recent = if context.recent_output.is_empty() {
            "(none)".to_string()
        } else {
            context.recent_output.join("\n")
        };

        format!(
            "Complete the following terminal command. Only output the completion, nothing else.\n\n\
             Context:\nCWD: {cwd}\nRecent output:\n{recent}\n\n\
             Command to complete: {}",
            context.input_prefix
        )
    }
}

impl AiProvider for OllamaProvider {
    fn complete(
        &self,
        context: &CompletionContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, AiError>> + Send + '_>> {
        let url = format!("{}/api/generate", self.base_url);
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: Self::format_prompt(context),
            stream: false,
        };
        let input_prefix = context.input_prefix.clone();

        Box::pin(async move {
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
        })
    }

    fn is_available(&self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> {
        let url = format!("{}/api/tags", self.base_url);
        Box::pin(async move {
            let client = reqwest::Client::builder()
                .timeout(AVAILABILITY_TIMEOUT)
                .build();
            let Ok(client) = client else {
                return false;
            };
            client
                .get(&url)
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
        })
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
        let context = CompletionContext {
            cwd: Some("/home/user".to_string()),
            input_prefix: "git sta".to_string(),
            recent_output: vec!["$ ls".to_string(), "file.txt".to_string()],
        };
        let prompt = OllamaProvider::format_prompt(&context);
        assert!(prompt.contains("git sta"));
        assert!(prompt.contains("/home/user"));
        assert!(prompt.contains("file.txt"));
    }

    #[test]
    fn test_format_prompt_no_cwd() {
        let context = CompletionContext {
            cwd: None,
            input_prefix: "ls".to_string(),
            recent_output: vec![],
        };
        let prompt = OllamaProvider::format_prompt(&context);
        assert!(prompt.contains("unknown"));
        assert!(prompt.contains("(none)"));
    }
}
