//! Error types for the terminal core.

use thiserror::Error;

/// Errors that can occur in the terminal core.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// PTY-related error.
    #[error("PTY error: {0}")]
    Pty(#[from] std::io::Error),

    /// Grid logic error.
    #[error("Grid error: {0}")]
    Grid(String),

    /// Rustix system call error.
    #[error("Rustix error: {0}")]
    Rustix(#[from] rustix::io::Errno),

    /// PTY spawn error.
    #[error("PTY spawn error: {0}")]
    PtySpawn(String),
}
