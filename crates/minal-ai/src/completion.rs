//! Command completion engine with debounce.

use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use crate::AiError;
use crate::ollama::OllamaProvider;
use crate::provider::CompletionContext;

/// Debounce interval for completion requests.
const DEBOUNCE_MS: u64 = 300;

/// Minimum input length required to trigger a completion request.
const MIN_INPUT_LEN: usize = 2;

/// State of the completion engine.
#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    /// Current ghost text suggestion.
    pub suggestion: Option<String>,
    /// Whether AI completion is enabled.
    pub enabled: bool,
}

/// Completion engine that manages AI-powered command suggestions.
pub struct CompletionEngine {
    provider: OllamaProvider,
    state: Arc<Mutex<CompletionState>>,
    last_request: Arc<Mutex<Option<Instant>>>,
}

impl CompletionEngine {
    /// Create a new completion engine with the given Ollama provider.
    pub fn new(provider: OllamaProvider) -> Self {
        Self {
            provider,
            state: Arc::new(Mutex::new(CompletionState {
                suggestion: None,
                enabled: true,
            })),
            last_request: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the current completion state.
    pub async fn state(&self) -> CompletionState {
        self.state.lock().await.clone()
    }

    /// Toggle AI completion on/off.
    pub async fn toggle(&self) {
        let mut state = self.state.lock().await;
        state.enabled = !state.enabled;
        if !state.enabled {
            state.suggestion = None;
        }
        tracing::info!(
            "AI completion {}",
            if state.enabled { "enabled" } else { "disabled" }
        );
    }

    /// Request a completion with debounce.
    ///
    /// Short inputs (fewer than 2 characters) are ignored. Requests that
    /// arrive within the debounce interval are silently dropped.
    pub async fn request_completion(&self, context: CompletionContext) -> Result<(), AiError> {
        {
            let state = self.state.lock().await;
            if !state.enabled {
                return Ok(());
            }
        }

        // Debounce: skip if too soon since last request.
        {
            let mut last = self.last_request.lock().await;
            let now = Instant::now();
            if let Some(prev) = *last {
                if now.duration_since(prev) < Duration::from_millis(DEBOUNCE_MS) {
                    return Ok(());
                }
            }
            *last = Some(now);
        }

        // Don't request for very short inputs.
        if context.input.trim().len() < MIN_INPUT_LEN {
            let mut state = self.state.lock().await;
            state.suggestion = None;
            return Ok(());
        }

        match self.provider.complete(&context).await {
            Ok(response) => {
                let mut state = self.state.lock().await;
                if response.text.is_empty() {
                    state.suggestion = None;
                } else {
                    state.suggestion = Some(response.text);
                }
            }
            Err(e) => {
                tracing::debug!("Completion request failed (Ollama may not be running): {e}");
                let mut state = self.state.lock().await;
                state.suggestion = None;
            }
        }

        Ok(())
    }

    /// Accept the current suggestion, returning it and clearing state.
    pub async fn accept(&self) -> Option<String> {
        let mut state = self.state.lock().await;
        state.suggestion.take()
    }

    /// Dismiss the current suggestion.
    pub async fn dismiss(&self) {
        let mut state = self.state.lock().await;
        state.suggestion = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> CompletionEngine {
        let provider = OllamaProvider::new(None, "codellama".to_string());
        CompletionEngine::new(provider)
    }

    #[tokio::test]
    async fn test_completion_state_default() {
        let engine = make_engine();
        let state = engine.state().await;
        assert!(state.enabled);
        assert!(state.suggestion.is_none());
    }

    #[tokio::test]
    async fn test_toggle() {
        let engine = make_engine();

        engine.toggle().await;
        assert!(!engine.state().await.enabled);

        engine.toggle().await;
        assert!(engine.state().await.enabled);
    }

    #[tokio::test]
    async fn test_dismiss() {
        let engine = make_engine();

        // Manually set a suggestion.
        {
            let mut state = engine.state.lock().await;
            state.suggestion = Some("test suggestion".to_string());
        }

        engine.dismiss().await;
        assert!(engine.state().await.suggestion.is_none());
    }

    #[tokio::test]
    async fn test_accept() {
        let engine = make_engine();

        // Manually set a suggestion.
        {
            let mut state = engine.state.lock().await;
            state.suggestion = Some("build --release".to_string());
        }

        let accepted = engine.accept().await;
        assert_eq!(accepted, Some("build --release".to_string()));
        assert!(engine.state().await.suggestion.is_none());
    }

    #[tokio::test]
    async fn test_short_input_clears_suggestion() {
        let engine = make_engine();

        let ctx = CompletionContext {
            input: "c".to_string(),
            ..Default::default()
        };

        // Should succeed even though Ollama is not running.
        let result = engine.request_completion(ctx).await;
        assert!(result.is_ok());
        assert!(engine.state().await.suggestion.is_none());
    }

    #[tokio::test]
    async fn test_disabled_skips_request() {
        let engine = make_engine();

        engine.toggle().await; // disable

        let ctx = CompletionContext {
            input: "cargo build".to_string(),
            ..Default::default()
        };

        let result = engine.request_completion(ctx).await;
        assert!(result.is_ok());
    }
}
