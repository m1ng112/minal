//! Error types for the plugin system.

use thiserror::Error;

/// Errors that can occur in the plugin system.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PluginError {
    /// Failed to read plugin manifest or WASM file.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Plugin manifest TOML parsing error.
    #[error("manifest parse error: {0}")]
    ManifestParse(#[from] toml::de::Error),

    /// Plugin manifest validation error.
    #[error("manifest validation error: {0}")]
    ManifestValidation(String),

    /// WASM compilation or instantiation error.
    #[error("WASM runtime error: {0}")]
    Runtime(String),

    /// Error calling a plugin-exported function.
    #[error("plugin call error: {0}")]
    Call(String),

    /// Plugin returned invalid data.
    #[error("invalid plugin response: {0}")]
    InvalidResponse(String),

    /// Plugin not found.
    #[error("plugin not found: {0}")]
    NotFound(String),

    /// Plugin is not loaded or has been unloaded.
    #[error("plugin not loaded: {0}")]
    NotLoaded(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Plugin directory does not exist.
    #[error("plugin directory not found: {0}")]
    DirNotFound(String),

    /// Failed to spawn a plugin worker thread.
    #[error("failed to spawn worker thread: {0}")]
    ThreadSpawn(#[source] std::io::Error),
}
