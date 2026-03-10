//! Top-level application errors.

use thiserror::Error;

/// Errors that can occur in the main application.
#[derive(Debug, Error)]
pub enum AppError {
    /// Window creation failed.
    #[error("Window creation failed: {0}")]
    WindowCreation(#[from] winit::error::OsError),

    /// Event loop initialization failed.
    #[error("Event loop error: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),

    /// Renderer initialization or rendering error.
    #[error("Renderer error: {0}")]
    Renderer(#[from] minal_renderer::RendererError),
}
