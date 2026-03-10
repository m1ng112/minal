//! `minal-renderer` — GPU rendering engine.
//!
//! Provides wgpu-based rendering pipelines for text, rectangles (backgrounds,
//! cursors), and UI overlays (AI panels).

mod error;
pub use error::RendererError;
