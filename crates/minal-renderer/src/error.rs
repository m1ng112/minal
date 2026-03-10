//! Error types for the renderer.

use thiserror::Error;

/// Errors that can occur in the rendering engine.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RendererError {
    /// Failed to initialize the GPU surface.
    #[error("Surface initialization failed: {0}")]
    SurfaceInit(String),

    /// Shader compilation error.
    #[error("Shader error: {0}")]
    Shader(String),
}
