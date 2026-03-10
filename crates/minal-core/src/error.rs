//! Error types for the terminal core.

use thiserror::Error;

/// Errors that can occur in the terminal core.
#[derive(Debug, Error)]
pub enum CoreError {
    /// PTY-related error.
    #[error("PTY error: {0}")]
    Pty(String),

    /// Grid operation error.
    #[error("Grid error: {0}")]
    Grid(String),
}
