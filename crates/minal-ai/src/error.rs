//! Error types for the AI engine.

use std::time::Duration;

use thiserror::Error;

/// Errors that can occur in the AI engine.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AiError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Provider returned an error.
    #[error("Provider error: {0}")]
    Provider(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Authentication failed (invalid or missing API key).
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Rate limited by the provider.
    #[error("Rate limited{}", .retry_after.map(|d| format!(", retry after {}s", d.as_secs())).unwrap_or_default())]
    RateLimited {
        /// Time to wait before retrying, if provided.
        retry_after: Option<Duration>,
    },

    /// Streaming error.
    #[error("Stream error: {0}")]
    StreamError(String),

    /// Request timed out.
    #[error("Request timed out")]
    Timeout,

    /// Keystore/credential storage error.
    #[error("Keystore error: {0}")]
    KeystoreError(String),

    /// Provider not available/reachable.
    #[error("Provider unavailable: {0}")]
    Unavailable(String),

    /// MCP transport error (process crash, connection failure).
    #[error("MCP transport error: {0}")]
    McpTransport(String),

    /// MCP protocol error (invalid response, initialization failure).
    #[error("MCP protocol error: {0}")]
    McpProtocol(String),

    /// MCP tool not found in registry.
    #[error("MCP tool not found: {0}")]
    McpToolNotFound(String),
}
