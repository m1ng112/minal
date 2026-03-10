//! Main application event loop.

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use minal_renderer::GpuContext;

/// Default window width in logical pixels.
const DEFAULT_WIDTH: u32 = 800;
/// Default window height in logical pixels.
const DEFAULT_HEIGHT: u32 = 600;
/// Window title.
const WINDOW_TITLE: &str = "Minal";

/// Default background color: Catppuccin Mocha base (#1e1e2e) normalized to 0.0-1.0.
const DEFAULT_BACKGROUND_COLOR: (f64, f64, f64) = (30.0 / 255.0, 30.0 / 255.0, 46.0 / 255.0);

/// Main application state implementing winit's `ApplicationHandler`.
///
/// Owns the window and GPU context. These are created lazily in the
/// `resumed` callback as required by winit 0.30's lifecycle model.
pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
}

impl App {
    /// Creates a new uninitialized application.
    pub fn new() -> Self {
        Self {
            window: None,
            gpu: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = match crate::window::create_window(
            event_loop,
            WINDOW_TITLE,
            DEFAULT_WIDTH,
            DEFAULT_HEIGHT,
        ) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to create window: {e}");
                event_loop.exit();
                return;
            }
        };

        let phys_size = window.inner_size();
        let scale_factor = window.scale_factor();
        tracing::info!(
            "Window created: {}x{} (scale factor: {:.2})",
            phys_size.width,
            phys_size.height,
            scale_factor
        );

        let gpu = match GpuContext::new(Arc::clone(&window)) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!("Failed to initialize GPU: {e}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.gpu = Some(gpu);

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    // Phase 1: single-window assumption. `_window_id` is not checked because
    // only one window is ever created. Multi-window support will require
    // dispatching events by window ID.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let (Some(window), Some(gpu)) = (self.window.as_ref(), self.gpu.as_mut()) else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                event_loop.exit();
            }

            WindowEvent::Resized(physical_size) => {
                gpu.resize(physical_size.width, physical_size.height);
                window.request_redraw();
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                let new_size = window.inner_size();
                gpu.resize(new_size.width, new_size.height);
                window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                let (r, g, b) = DEFAULT_BACKGROUND_COLOR;
                match gpu.render_clear(r, g, b) {
                    Ok(()) => {}
                    Err(minal_renderer::RendererError::SurfaceOutdated) => {
                        let size = window.inner_size();
                        gpu.resize(size.width, size.height);
                        window.request_redraw();
                    }
                    Err(minal_renderer::RendererError::SurfaceTimeout) => {
                        tracing::debug!("Surface texture timeout, retrying next frame");
                        window.request_redraw();
                    }
                    Err(e) => {
                        // SurfaceLost, OutOfMemory, and other errors are fatal.
                        // Surface recreation is not yet implemented.
                        tracing::error!("Render error: {e}");
                        event_loop.exit();
                    }
                }
            }

            _ => {}
        }
    }
}
