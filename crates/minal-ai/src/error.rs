//! Error types for the AI engine.

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
}
