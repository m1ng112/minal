//! `minal-renderer` — GPU rendering engine.
//!
//! Provides wgpu-based rendering pipelines for text, rectangles (backgrounds,
//! cursors), and UI overlays (AI panels).

pub mod atlas;
pub mod chat_panel;
mod context;
mod error;
pub mod rect;
pub mod renderer;
pub mod text;

pub use chat_panel::{ChatHitRegion, ChatMessage, ChatRole};
pub use context::{GpuContext, SurfaceFrame};
pub use error::RendererError;
pub use renderer::{Renderer, TabBarInfo, Viewport};

#[cfg(test)]
mod test_harness;
