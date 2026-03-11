//! Error types for configuration management.

use thiserror::Error;

/// Errors that can occur in configuration handling.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Failed to read config file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parsing error.
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),

    /// Invalid color value.
    #[error("Invalid color value: {0}")]
    InvalidColor(String),
}
