//! Main application event loop.

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use minal_core::ansi::Color;
use minal_core::term::Terminal;
use minal_renderer::{GpuContext, Renderer, RendererError};

/// Default window width in logical pixels.
const DEFAULT_WIDTH: u32 = 800;
/// Default window height in logical pixels.
const DEFAULT_HEIGHT: u32 = 600;
/// Window title.
const WINDOW_TITLE: &str = "Minal";

/// Default terminal size.
const DEFAULT_COLS: usize = 80;
/// Default terminal rows.
const DEFAULT_ROWS: usize = 24;

/// Main application state implementing winit's `ApplicationHandler`.
///
/// Owns the window, GPU context, terminal state, and renderer.
pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    terminal: Option<Terminal>,
}

impl App {
    /// Creates a new uninitialized application.
    pub fn new() -> Self {
        Self {
            window: None,
            gpu: None,
            renderer: None,
            terminal: None,
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

        let renderer = match Renderer::new(gpu.device(), gpu.queue(), gpu.config().format) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create renderer: {e}");
                event_loop.exit();
                return;
            }
        };

        // Create terminal with dummy content for visual validation.
        let mut terminal = Terminal::new(DEFAULT_ROWS, DEFAULT_COLS);
        populate_dummy_content(&mut terminal);

        self.window = Some(window);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.terminal = Some(terminal);

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
        let (Some(window), Some(gpu), Some(renderer), Some(terminal)) = (
            self.window.as_ref(),
            self.gpu.as_mut(),
            self.renderer.as_mut(),
            self.terminal.as_ref(),
        ) else {
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
                let frame = match gpu.begin_frame() {
                    Ok(f) => f,
                    Err(RendererError::SurfaceOutdated) => {
                        let size = window.inner_size();
                        gpu.resize(size.width, size.height);
                        window.request_redraw();
                        return;
                    }
                    Err(RendererError::SurfaceTimeout) => {
                        tracing::debug!("Surface texture timeout, retrying next frame");
                        window.request_redraw();
                        return;
                    }
                    Err(e) => {
                        tracing::error!("Render error: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                let (w, h) = gpu.size();
                renderer.render(
                    gpu.device(),
                    gpu.queue(),
                    &frame.view,
                    w,
                    h,
                    terminal.grid(),
                    terminal.cursor(),
                );

                frame.present();
            }

            _ => {}
        }
    }
}

/// Fills the terminal grid with dummy content for visual validation.
fn populate_dummy_content(terminal: &mut Terminal) {
    let cols = terminal.cols();
    let rows = terminal.rows();

    let demo_lines = [
        "Minal - AI-first Terminal Emulator",
        "",
        "$ echo \"Hello, World!\"",
        "Hello, World!",
        "",
        "$ ls -la",
        "drwxr-xr-x  5 user staff  160 Mar 11 10:00 .",
        "drwxr-xr-x 20 user staff  640 Mar 11 09:55 ..",
        "-rw-r--r--  1 user staff 1234 Mar 11 10:00 Cargo.toml",
        "-rw-r--r--  1 user staff  567 Mar 11 10:00 README.md",
        "drwxr-xr-x  4 user staff  128 Mar 11 10:00 src",
        "",
        "$ cargo build --release",
        "   Compiling minal v0.1.0",
        "    Finished release [optimized] target(s)",
        "",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "abcdefghijklmnopqrstuvwxyz",
        "0123456789 !@#$%^&*()_+-=[]{}|;':\",./<>?",
        "",
        "The quick brown fox jumps over the lazy dog.",
        "",
        "Japanese: \u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{4e16}\u{754c}",
        "Ready.",
    ];

    let grid = terminal.grid_mut();

    for (row_idx, line) in demo_lines.iter().enumerate() {
        if row_idx >= rows {
            break;
        }
        let Some(row) = grid.row_mut(row_idx) else {
            continue;
        };

        for (col_idx, ch) in line.chars().enumerate() {
            if col_idx >= cols {
                break;
            }
            let Some(cell) = row.get_mut(col_idx) else {
                continue;
            };
            cell.c = ch;

            // Color the prompt lines green.
            if line.starts_with("$ ") && col_idx < 2 {
                cell.fg = Color::Named(minal_core::ansi::NamedColor::Green);
            }
            // Color "Compiling" and "Finished" lines.
            if line.contains("Compiling") || line.contains("Finished") {
                cell.fg = Color::Named(minal_core::ansi::NamedColor::Cyan);
            }
            // Color the title line.
            if row_idx == 0 {
                cell.fg = Color::Named(minal_core::ansi::NamedColor::Blue);
            }
        }
    }

    // Place cursor at the end of "Ready."
    // TODO(pty): Remove hardcoded position once PTY integration is added.
    let cursor = terminal.cursor_mut();
    cursor.row = 23;
    cursor.col = 6;
    cursor.visible = true;
}
