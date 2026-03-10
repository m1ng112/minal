//! `minal-renderer` — GPU rendering engine.
//!
//! Provides wgpu-based rendering pipelines for text, rectangles (backgrounds,
//! cursors), and UI overlays (AI panels).

mod context;
mod error;

pub use context::GpuContext;
pub use error::RendererError;

#[cfg(test)]
mod test_harness;
