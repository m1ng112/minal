//! AI provider abstraction.

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::error::AiError;
use crate::types::{AiContext, ErrorAnalysis, ErrorContext, Message};

/// Trait for AI providers (completion, chat, error analysis).
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Generate a single-turn completion suggestion.
    async fn complete(&self, context: &AiContext) -> Result<String, AiError>;

    /// Stream a multi-turn chat response token by token.
    async fn chat_stream(
        &self,
        messages: &[Message],
        context: &AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError>;

    /// Analyze a command error and suggest fixes.
    async fn analyze_error(&self, error: &ErrorContext) -> Result<ErrorAnalysis, AiError>;

    /// Check if this provider is currently available/reachable.
    async fn is_available(&self) -> bool;

    /// Human-readable name of this provider (e.g., "ollama", "anthropic", "openai").
    fn name(&self) -> &str;
}
