//! Error types for configuration management.

use thiserror::Error;

/// Errors that can occur in configuration handling.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read config file.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// TOML parsing error.
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),

    /// Configuration value validation failed.
    #[error("Validation error: {0}")]
    Validation(String),

    /// Could not determine configuration directory.
    #[error("Could not determine configuration directory")]
    ConfigDir,
}
