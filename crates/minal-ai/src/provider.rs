//! AI provider abstraction.

use crate::AiError;
use std::future::Future;
use std::pin::Pin;

/// Context for AI completion requests.
#[derive(Debug, Clone)]
pub struct CompletionContext {
    /// Current working directory, if known.
    pub cwd: Option<String>,
    /// Text the user has typed on the current line (after the prompt).
    pub input_prefix: String,
    /// Recent terminal output lines for context.
    pub recent_output: Vec<String>,
}

/// Trait for AI completion providers.
pub trait AiProvider: Send + Sync {
    /// Generate a completion suggestion given a context.
    fn complete(
        &self,
        context: &CompletionContext,
    ) -> Pin<Box<dyn Future<Output = Result<String, AiError>> + Send + '_>>;

    /// Check if the provider is available.
    fn is_available(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
}
