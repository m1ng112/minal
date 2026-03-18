//! Main application event loop.
//!
//! Integrates the 3-thread architecture:
//! - **Main thread**: winit event loop + wgpu rendering
//! - **I/O thread**: tokio runtime for async PTY read/write + VT parsing
//!
//! Communication:
//! - Main -> I/O: crossbeam-channel [`Sender<IoEvent>`]
//! - I/O -> Main: winit [`EventLoopProxy<WakeupReason>`]
//! - Shared state: [`Arc<Mutex<Terminal>>`]

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use minal_core::ansi::Mode;
use minal_core::handler::Handler;
use minal_core::pty::{AsyncPty, Pty, PtySize};
use minal_core::term::Terminal;
use minal_renderer::{GpuContext, Renderer, RendererError};

use crate::event::{IoEvent, WakeupReason};

/// Default window width in logical pixels.
const DEFAULT_WIDTH: u32 = 800;
/// Default window height in logical pixels.
const DEFAULT_HEIGHT: u32 = 600;
/// Window title.
const WINDOW_TITLE: &str = "Minal";

/// Cursor blink interval in milliseconds.
const CURSOR_BLINK_MS: u64 = 600;

/// Main application state implementing winit's `ApplicationHandler`.
///
/// Owns the window, GPU context, terminal state, and renderer. Manages
/// the I/O thread and inter-thread communication channels.
pub struct App {
    proxy: EventLoopProxy<WakeupReason>,
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    terminal: Option<Arc<Mutex<Terminal>>>,
    io_tx: Option<Sender<IoEvent>>,
    io_thread: Option<JoinHandle<()>>,
    /// Whether the cursor blink is currently in the visible phase.
    cursor_visible: bool,
    /// Timestamp of the last cursor blink toggle.
    last_blink: Instant,
}

impl App {
    /// Creates a new uninitialized application with the given event loop proxy.
    pub fn new(proxy: EventLoopProxy<WakeupReason>) -> Self {
        Self {
            proxy,
            window: None,
            gpu: None,
            renderer: None,
            terminal: None,
            io_tx: None,
            io_thread: None,
            cursor_visible: true,
            last_blink: Instant::now(),
        }
    }

    /// Compute terminal grid dimensions from window size and cell metrics,
    /// accounting for window padding on all sides.
    fn compute_grid_size(
        window_width: u32,
        window_height: u32,
        cell_width: f32,
        cell_height: f32,
        padding: f32,
    ) -> (usize, usize) {
        let usable_width = (window_width as f32 - padding * 2.0).max(0.0);
        let usable_height = (window_height as f32 - padding * 2.0).max(0.0);
        let cols = if cell_width > 0.0 {
            (usable_width / cell_width).floor() as usize
        } else {
            80
        };
        let rows = if cell_height > 0.0 {
            (usable_height / cell_height).floor() as usize
        } else {
            24
        };
        (rows.max(1), cols.max(1))
    }

    /// Send an I/O event to the I/O thread, logging on failure.
    fn send_io_event(&self, event: IoEvent) {
        if let Some(ref tx) = self.io_tx {
            if let Err(e) = tx.send(event) {
                tracing::warn!("Failed to send I/O event: {e}");
            }
        }
    }

    /// Translate a keyboard event to bytes to send to the PTY.
    fn translate_key_input(&self, event: &winit::event::KeyEvent) -> Option<Vec<u8>> {
        if event.state != ElementState::Pressed {
            return None;
        }

        // Check if the terminal is in application cursor key mode.
        let app_cursor = self
            .terminal
            .as_ref()
            .and_then(|t| t.lock().ok())
            .is_some_and(|t| t.mode(Mode::CursorKeys));

        match &event.logical_key {
            Key::Named(named) => {
                let bytes = match named {
                    NamedKey::Enter => b"\r".to_vec(),
                    NamedKey::Backspace => vec![0x7f],
                    NamedKey::Tab => b"\t".to_vec(),
                    NamedKey::Escape => vec![0x1b],
                    NamedKey::ArrowUp => {
                        if app_cursor {
                            b"\x1bOA".to_vec()
                        } else {
                            b"\x1b[A".to_vec()
                        }
                    }
                    NamedKey::ArrowDown => {
                        if app_cursor {
                            b"\x1bOB".to_vec()
                        } else {
                            b"\x1b[B".to_vec()
                        }
                    }
                    NamedKey::ArrowRight => {
                        if app_cursor {
                            b"\x1bOC".to_vec()
                        } else {
                            b"\x1b[C".to_vec()
                        }
                    }
                    NamedKey::ArrowLeft => {
                        if app_cursor {
                            b"\x1bOD".to_vec()
                        } else {
                            b"\x1b[D".to_vec()
                        }
                    }
                    NamedKey::Home => b"\x1b[H".to_vec(),
                    NamedKey::End => b"\x1b[F".to_vec(),
                    NamedKey::PageUp => b"\x1b[5~".to_vec(),
                    NamedKey::PageDown => b"\x1b[6~".to_vec(),
                    NamedKey::Delete => b"\x1b[3~".to_vec(),
                    NamedKey::Insert => b"\x1b[2~".to_vec(),
                    NamedKey::F1 => b"\x1bOP".to_vec(),
                    NamedKey::F2 => b"\x1bOQ".to_vec(),
                    NamedKey::F3 => b"\x1bOR".to_vec(),
                    NamedKey::F4 => b"\x1bOS".to_vec(),
                    NamedKey::F5 => b"\x1b[15~".to_vec(),
                    NamedKey::F6 => b"\x1b[17~".to_vec(),
                    NamedKey::F7 => b"\x1b[18~".to_vec(),
                    NamedKey::F8 => b"\x1b[19~".to_vec(),
                    NamedKey::F9 => b"\x1b[20~".to_vec(),
                    NamedKey::F10 => b"\x1b[21~".to_vec(),
                    NamedKey::F11 => b"\x1b[23~".to_vec(),
                    NamedKey::F12 => b"\x1b[24~".to_vec(),
                    _ => return None,
                };
                Some(bytes)
            }
            Key::Character(text) => {
                // winit provides pre-composed text via SmolStr.
                let s = text.as_str();
                if s.is_empty() {
                    return None;
                }
                Some(s.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    /// Shut down the I/O thread and clean up.
    fn shutdown(&mut self) {
        self.send_io_event(IoEvent::Shutdown);
        if let Some(handle) = self.io_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ApplicationHandler<WakeupReason> for App {
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

        // Load configuration.
        let config = minal_config::Config::load().unwrap_or_else(|e| {
            tracing::warn!("Failed to load config: {e}, using defaults");
            minal_config::Config::default()
        });

        let gpu = match GpuContext::new(Arc::clone(&window)) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!("Failed to initialize GPU: {e}");
                event_loop.exit();
                return;
            }
        };

        let renderer = match Renderer::new(gpu.device(), gpu.queue(), gpu.config().format, &config)
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create renderer: {e}");
                event_loop.exit();
                return;
            }
        };

        // Compute terminal dimensions from window size and cell metrics.
        let (cell_width, cell_height) = renderer.cell_size();
        let padding = renderer.padding();
        let (rows, cols) = Self::compute_grid_size(
            phys_size.width,
            phys_size.height,
            cell_width,
            cell_height,
            padding,
        );
        tracing::info!("Terminal grid: {rows}x{cols} (cell: {cell_width:.1}x{cell_height:.1})");

        let terminal = Arc::new(Mutex::new(Terminal::new(rows, cols)));

        // Open PTY and spawn the I/O thread.
        let shell = config.shell.resolve_program();
        let pty_size = PtySize::new(rows as u16, cols as u16);

        let pty = match Pty::open(&shell, pty_size, &[]) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to open PTY: {e}");
                event_loop.exit();
                return;
            }
        };
        tracing::info!(child_pid = pty.child_pid(), shell = %shell, "PTY opened");

        // Create crossbeam channel for Main -> I/O communication.
        let (io_tx, io_rx) = crossbeam_channel::unbounded::<IoEvent>();

        // Spawn the I/O thread.
        let terminal_clone = Arc::clone(&terminal);
        let proxy_clone = self.proxy.clone();
        let io_thread = std::thread::Builder::new()
            .name("minal-io".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("Failed to create tokio runtime: {e}");
                        return;
                    }
                };
                rt.block_on(io_loop(pty, io_rx, terminal_clone, proxy_clone));
            });

        match io_thread {
            Ok(handle) => {
                self.io_thread = Some(handle);
            }
            Err(e) => {
                tracing::error!("Failed to spawn I/O thread: {e}");
                event_loop.exit();
                return;
            }
        }

        self.window = Some(window);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.terminal = Some(terminal);
        self.io_tx = Some(io_tx);
        self.cursor_visible = true;
        self.last_blink = Instant::now();

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WakeupReason) {
        match event {
            WakeupReason::TerminalUpdated => {
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::ChildExited(code) => {
                tracing::info!("Child process exited with code {code}");
                self.shutdown();
                event_loop.exit();
            }
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
        let (Some(window), Some(gpu), Some(renderer)) = (
            self.window.as_ref(),
            self.gpu.as_mut(),
            self.renderer.as_mut(),
        ) else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                self.shutdown();
                event_loop.exit();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(bytes) = self.translate_key_input(&event) {
                    self.send_io_event(IoEvent::Input(bytes));
                    // Reset cursor blink to visible on input.
                    self.cursor_visible = true;
                    self.last_blink = Instant::now();
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::Resized(physical_size) => {
                handle_resize(
                    gpu,
                    renderer,
                    &self.terminal,
                    &self.io_tx,
                    physical_size.width,
                    physical_size.height,
                );
                window.request_redraw();
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                let new_size = window.inner_size();
                handle_resize(
                    gpu,
                    renderer,
                    &self.terminal,
                    &self.io_tx,
                    new_size.width,
                    new_size.height,
                );
                window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                // Update cursor blink state.
                let now = Instant::now();
                let blink_interval = Duration::from_millis(CURSOR_BLINK_MS);
                if now.duration_since(self.last_blink) >= blink_interval {
                    self.cursor_visible = !self.cursor_visible;
                    self.last_blink = now;
                }

                let Some(ref terminal) = self.terminal else {
                    return;
                };

                let Ok(mut term) = terminal.lock() else {
                    tracing::warn!("Failed to lock terminal for rendering");
                    return;
                };

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

                // Create a cursor copy with blink state applied.
                let mut cursor = term.cursor().clone();
                if !self.cursor_visible {
                    cursor.visible = false;
                }

                renderer.render(
                    gpu.device(),
                    gpu.queue(),
                    &frame.view,
                    w,
                    h,
                    term.grid(),
                    &cursor,
                );

                term.clear_dirty();
                // Drop the lock before present to minimize lock hold time.
                drop(term);

                frame.present();

                // Schedule the next blink wakeup.
                let next_blink = self.last_blink + blink_interval;
                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_blink));
            }

            _ => {}
        }
    }
}

/// Handle a window resize: update GPU surface, terminal grid, and PTY.
fn handle_resize(
    gpu: &mut GpuContext,
    renderer: &Renderer,
    terminal: &Option<Arc<Mutex<Terminal>>>,
    io_tx: &Option<Sender<IoEvent>>,
    width: u32,
    height: u32,
) {
    gpu.resize(width, height);

    let (cell_width, cell_height) = renderer.cell_size();
    let padding = renderer.padding();
    let (rows, cols) = App::compute_grid_size(width, height, cell_width, cell_height, padding);

    if let Some(terminal) = terminal {
        if let Ok(mut term) = terminal.lock() {
            term.resize(rows, cols);
        }
    }

    let pty_size = PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: width as u16,
        pixel_height: height as u16,
    };
    if let Some(tx) = io_tx {
        if let Err(e) = tx.send(IoEvent::Resize(pty_size)) {
            tracing::warn!("Failed to send resize event: {e}");
        }
    }
}

/// The async I/O loop running on the I/O thread.
///
/// Reads PTY output, feeds it through the VT parser to update terminal state,
/// and listens for commands from the main thread.
async fn io_loop(
    pty: Pty,
    io_rx: crossbeam_channel::Receiver<IoEvent>,
    terminal: Arc<Mutex<Terminal>>,
    proxy: EventLoopProxy<WakeupReason>,
) {
    let async_pty = match AsyncPty::from_pty(pty) {
        Ok(ap) => ap,
        Err(e) => {
            tracing::error!("Failed to create AsyncPty: {e}");
            let _ = proxy.send_event(WakeupReason::ChildExited(1));
            return;
        }
    };

    let mut parser = vte::Parser::new();
    let mut read_buf = [0u8; 8192];

    // Bridge crossbeam Receiver to tokio mpsc so we can use tokio::select!.
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<IoEvent>();
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = io_rx.recv() {
            let is_shutdown = matches!(event, IoEvent::Shutdown);
            if cmd_tx.send(event).is_err() {
                break;
            }
            if is_shutdown {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            result = async_pty.read(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        // EOF: child closed the slave side.
                        tracing::info!("PTY EOF, child process ended");
                        let code = async_pty.try_wait()
                            .ok()
                            .flatten()
                            .unwrap_or(0);
                        let _ = proxy.send_event(WakeupReason::ChildExited(code));
                        return;
                    }
                    Ok(n) => {
                        // Feed data through VT parser into terminal state.
                        if let Ok(mut term) = terminal.lock() {
                            let mut handler = Handler::new(&mut term);
                            for &byte in &read_buf[..n] {
                                parser.advance(&mut handler, byte);
                            }
                            // Only notify main thread if we actually updated state.
                            drop(term);
                            let _ = proxy.send_event(WakeupReason::TerminalUpdated);
                        }
                    }
                    Err(e) => {
                        tracing::error!("PTY read error: {e}");
                        let code = async_pty.try_wait()
                            .ok()
                            .flatten()
                            .unwrap_or(1);
                        let _ = proxy.send_event(WakeupReason::ChildExited(code));
                        return;
                    }
                }
            }

            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(IoEvent::Input(data)) => {
                        let mut offset = 0;
                        while offset < data.len() {
                            match async_pty.write(&data[offset..]).await {
                                Ok(n) => offset += n,
                                Err(e) => {
                                    tracing::error!("PTY write error: {e}");
                                    break;
                                }
                            }
                        }
                    }
                    Some(IoEvent::Resize(size)) => {
                        if let Err(e) = async_pty.resize(size) {
                            tracing::warn!("PTY resize failed: {e}");
                        }
                    }
                    Some(IoEvent::Shutdown) | None => {
                        tracing::info!("I/O thread shutting down");
                        return;
                    }
                }
            }
        }
    }
}
