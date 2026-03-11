//! AI provider abstraction types.

/// Context information sent with completion requests.
#[derive(Debug, Clone, Default)]
pub struct CompletionContext {
    /// Current working directory.
    pub cwd: Option<String>,
    /// Current command line input.
    pub input: String,
    /// Recent command history (most recent first).
    pub history: Vec<String>,
}

/// Result of a completion request.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// The generated completion text.
    pub text: String,
}
