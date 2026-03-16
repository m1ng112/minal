//! Error types for the terminal core.

use thiserror::Error;

/// Errors that can occur in the terminal core.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// PTY I/O error.
    #[error("PTY error: {0}")]
    Pty(#[from] std::io::Error),

    /// Fork failed.
    #[error("fork failed: {0}")]
    ForkFailed(String),

    /// PTY setup error (openpt, grantpt, setsid, controlling terminal, etc.).
    #[error("PTY setup error: {0}")]
    PtySetup(String),

    /// Terminal resize error.
    #[error("PTY resize error: {0}")]
    Resize(String),

    /// Grid logic error.
    #[error("Grid error: {0}")]
    Grid(String),
}
