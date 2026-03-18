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

    /// No suitable GPU adapter was found.
    #[error("No suitable GPU adapter found")]
    AdapterNotFound,

    /// Failed to request a GPU device.
    #[error("Device request failed: {0}")]
    DeviceRequest(String),

    /// Surface configuration failed.
    #[error("Surface configuration failed: {0}")]
    SurfaceConfig(String),

    /// Surface texture is outdated and needs reconfiguration.
    #[error("Surface texture outdated")]
    SurfaceOutdated,

    /// Surface was lost and may need recreation.
    #[error("Surface lost")]
    SurfaceLost,

    /// GPU out of memory.
    #[error("GPU out of memory")]
    OutOfMemory,

    /// Surface texture acquisition timed out (transient, may retry).
    #[error("Surface texture acquire timeout")]
    SurfaceTimeout,

    /// A generic surface error from the backend.
    #[error("Surface error: {0}")]
    SurfaceOther(String),

    /// Buffer mapping failed during texture readback.
    #[error("Buffer map failed: {0}")]
    BufferMap(String),
}
