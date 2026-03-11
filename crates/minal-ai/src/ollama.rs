//! Ollama provider for local AI completion.

use serde::{Deserialize, Serialize};

use crate::AiError;
use crate::provider::{CompletionContext, CompletionResponse};

/// Default Ollama API endpoint.
const DEFAULT_ENDPOINT: &str = "http://localhost:11434";

#[derive(Debug, Clone, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: GenerateOptions,
}

#[derive(Debug, Clone, Serialize)]
struct GenerateOptions {
    num_predict: u32,
    temperature: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct GenerateResponse {
    response: String,
    #[allow(dead_code)]
    done: bool,
}

/// Ollama AI provider that communicates with a local Ollama instance.
pub struct OllamaProvider {
    client: reqwest::Client,
    endpoint: String,
    model: String,
}

impl OllamaProvider {
    /// Create a new Ollama provider.
    ///
    /// If `endpoint` is `None`, defaults to `http://localhost:11434`.
    pub fn new(endpoint: Option<String>, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            client,
            endpoint: endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            model,
        }
    }

    /// Check if the Ollama server is available.
    pub async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.endpoint))
            .send()
            .await
            .is_ok()
    }

    /// Generate a completion for the given context.
    pub async fn complete(
        &self,
        context: &CompletionContext,
    ) -> Result<CompletionResponse, AiError> {
        let prompt = build_prompt(context);

        let request = GenerateRequest {
            model: self.model.clone(),
            prompt,
            stream: false,
            options: GenerateOptions {
                num_predict: 64,
                temperature: 0.2,
            },
        };

        let url = format!("{}/api/generate", self.endpoint);
        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            return Err(AiError::Provider(format!(
                "Ollama returned status {}",
                response.status()
            )));
        }

        let gen_response: GenerateResponse = response.json().await?;

        // Clean up the response — take only the first line.
        let text = gen_response
            .response
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(CompletionResponse { text })
    }
}

/// Build a prompt for command completion from context.
fn build_prompt(context: &CompletionContext) -> String {
    let mut prompt = String::new();

    if let Some(ref cwd) = context.cwd {
        prompt.push_str(&format!("Current directory: {cwd}\n"));
    }

    if !context.history.is_empty() {
        prompt.push_str("Recent commands:\n");
        for cmd in context.history.iter().take(5) {
            prompt.push_str(&format!("  {cmd}\n"));
        }
    }

    prompt.push_str(&format!(
        "Complete the following shell command (respond with ONLY the completion, no explanation):\n{}",
        context.input
    ));

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_with_context() {
        let ctx = CompletionContext {
            cwd: Some("/home/user".to_string()),
            input: "cargo b".to_string(),
            history: vec!["cargo test".to_string(), "cargo build".to_string()],
        };
        let prompt = build_prompt(&ctx);
        assert!(prompt.contains("cargo b"));
        assert!(prompt.contains("/home/user"));
        assert!(prompt.contains("cargo test"));
    }

    #[test]
    fn test_build_prompt_minimal() {
        let ctx = CompletionContext {
            cwd: None,
            input: "ls".to_string(),
            history: vec![],
        };
        let prompt = build_prompt(&ctx);
        assert!(prompt.contains("ls"));
        assert!(!prompt.contains("Current directory"));
        assert!(!prompt.contains("Recent commands"));
    }

    #[test]
    fn test_build_prompt_limits_history() {
        let ctx = CompletionContext {
            cwd: None,
            input: "git".to_string(),
            history: (0..10).map(|i| format!("cmd-{i}")).collect(),
        };
        let prompt = build_prompt(&ctx);
        assert!(prompt.contains("cmd-0"));
        assert!(prompt.contains("cmd-4"));
        assert!(!prompt.contains("cmd-5"));
    }

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new(None, "codellama".to_string());
        assert_eq!(provider.endpoint, DEFAULT_ENDPOINT);
        assert_eq!(provider.model, "codellama");
    }

    #[test]
    fn test_ollama_custom_endpoint() {
        let provider = OllamaProvider::new(
            Some("http://localhost:8080".to_string()),
            "llama2".to_string(),
        );
        assert_eq!(provider.endpoint, "http://localhost:8080");
        assert_eq!(provider.model, "llama2");
    }
}
