//! Main application event loop.
//!
//! Implements winit's `ApplicationHandler` and manages the three-thread
//! architecture: main thread (events), I/O thread (PTY + VT parsing),
//! and renderer (wgpu draw).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use minal_core::handler::Handler;
use minal_core::pty::Pty;
use minal_core::term::Terminal;
use minal_renderer::{GpuContext, Renderer, RendererError};

use crate::event::{IoAction, MainEvent};

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

/// PTY read buffer size.
const PTY_READ_BUF_SIZE: usize = 65536;

/// Main application state implementing winit's `ApplicationHandler`.
///
/// Owns the window, GPU context, renderer, and communication channels
/// to the I/O thread. Terminal state is shared via `Arc<Mutex<Terminal>>`.
pub struct App {
    proxy: EventLoopProxy<()>,
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    terminal: Option<Arc<Mutex<Terminal>>>,
    io_tx: Option<Sender<IoAction>>,
    main_rx: Option<Receiver<MainEvent>>,
    io_thread: Option<std::thread::JoinHandle<()>>,
}

impl App {
    /// Creates a new uninitialized application.
    pub fn new(proxy: EventLoopProxy<()>) -> Self {
        Self {
            proxy,
            window: None,
            gpu: None,
            renderer: None,
            terminal: None,
            io_tx: None,
            main_rx: None,
            io_thread: None,
        }
    }

    /// Drain pending events from the I/O thread and act on them.
    fn process_io_events(&mut self, event_loop: &ActiveEventLoop) {
        let Some(main_rx) = self.main_rx.as_ref() else {
            return;
        };

        while let Ok(event) = main_rx.try_recv() {
            match event {
                MainEvent::Redraw => {
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                }
                MainEvent::ChildExited(code) => {
                    tracing::info!("Child process exited with code: {:?}", code);
                    event_loop.exit();
                }
                MainEvent::TitleChanged(title) => {
                    if let Some(ref w) = self.window {
                        w.set_title(&title);
                    }
                }
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Send shutdown signal to I/O thread
        if let Some(ref tx) = self.io_tx {
            let _ = tx.send(IoAction::Shutdown);
        }
        // Wait for I/O thread to finish
        if let Some(handle) = self.io_thread.take() {
            let _ = handle.join();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Create window
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

        // Initialize GPU
        let gpu = match GpuContext::new(Arc::clone(&window)) {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::error!("Failed to initialize GPU: {e}");
                event_loop.exit();
                return;
            }
        };

        // Initialize renderer
        let renderer = match Renderer::new(gpu.device(), gpu.queue(), gpu.config().format) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create renderer: {e}");
                event_loop.exit();
                return;
            }
        };

        // Create shared terminal state
        let terminal = Arc::new(Mutex::new(Terminal::new(DEFAULT_ROWS, DEFAULT_COLS)));

        // Load config to get shell
        let config = minal_config::Config::load().unwrap_or_default();
        let shell = config.shell.program.clone();
        let args = config.shell.args.clone();
        tracing::info!("Spawning shell: {} {:?}", shell, args);

        // Spawn PTY
        let pty = match Pty::spawn(&shell, &args, DEFAULT_ROWS as u16, DEFAULT_COLS as u16) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to spawn PTY: {e}");
                event_loop.exit();
                return;
            }
        };

        // Set non-blocking for the I/O loop
        if let Err(e) = pty.set_nonblocking() {
            tracing::error!("Failed to set PTY non-blocking: {e}");
            event_loop.exit();
            return;
        }

        // Create channels for inter-thread communication
        let (io_tx, io_rx) = crossbeam_channel::unbounded::<IoAction>();
        let (main_tx, main_rx) = crossbeam_channel::unbounded::<MainEvent>();

        // Spawn I/O thread
        let io_terminal = Arc::clone(&terminal);
        let proxy = self.proxy.clone();
        let io_handle = std::thread::Builder::new()
            .name("minal-io".into())
            .spawn(move || {
                run_io_thread(pty, io_terminal, io_rx, main_tx, proxy);
            })
            .expect("failed to spawn I/O thread");

        self.window = Some(window);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.terminal = Some(terminal);
        self.io_tx = Some(io_tx);
        self.main_rx = Some(main_rx);
        self.io_thread = Some(io_handle);

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.process_io_events(event_loop);
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
        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                // Send shutdown to I/O thread
                if let Some(ref tx) = self.io_tx {
                    let _ = tx.send(IoAction::Shutdown);
                }
                event_loop.exit();
            }

            WindowEvent::Resized(physical_size) => {
                if let Some(ref mut gpu) = self.gpu {
                    gpu.resize(physical_size.width, physical_size.height);
                }
                // TODO: Calculate rows/cols from pixel size and cell size,
                // then send IoAction::Resize. For now we keep fixed terminal size.
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(ref w) = self.window {
                    let new_size = w.inner_size();
                    if let Some(ref mut gpu) = self.gpu {
                        gpu.resize(new_size.width, new_size.height);
                    }
                    w.request_redraw();
                }
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state == ElementState::Pressed {
                    if let Some(bytes) = key_to_bytes(&key_event.logical_key, &key_event.text) {
                        if let Some(ref tx) = self.io_tx {
                            let _ = tx.send(IoAction::PtyWrite(bytes));
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let (Some(window), Some(gpu), Some(renderer), Some(terminal)) = (
                    self.window.as_ref(),
                    self.gpu.as_mut(),
                    self.renderer.as_mut(),
                    self.terminal.as_ref(),
                ) else {
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
                let term = terminal.lock().expect("terminal lock poisoned");
                renderer.render(
                    gpu.device(),
                    gpu.queue(),
                    &frame.view,
                    w,
                    h,
                    term.grid(),
                    term.cursor(),
                );
                drop(term);

                frame.present();
            }

            _ => {}
        }
    }
}

/// Convert a winit key event to bytes to write to the PTY.
fn key_to_bytes(key: &Key, text: &Option<winit::keyboard::SmolStr>) -> Option<Vec<u8>> {
    // First, check for named keys that map to control sequences
    if let Key::Named(named) = key {
        let bytes: Option<Vec<u8>> = match named {
            NamedKey::Enter => Some(vec![b'\r']),
            NamedKey::Backspace => Some(vec![0x7f]),
            NamedKey::Tab => Some(vec![b'\t']),
            NamedKey::Escape => Some(vec![0x1b]),
            NamedKey::ArrowUp => Some(b"\x1b[A".to_vec()),
            NamedKey::ArrowDown => Some(b"\x1b[B".to_vec()),
            NamedKey::ArrowRight => Some(b"\x1b[C".to_vec()),
            NamedKey::ArrowLeft => Some(b"\x1b[D".to_vec()),
            NamedKey::Home => Some(b"\x1b[H".to_vec()),
            NamedKey::End => Some(b"\x1b[F".to_vec()),
            NamedKey::PageUp => Some(b"\x1b[5~".to_vec()),
            NamedKey::PageDown => Some(b"\x1b[6~".to_vec()),
            NamedKey::Insert => Some(b"\x1b[2~".to_vec()),
            NamedKey::Delete => Some(b"\x1b[3~".to_vec()),
            _ => None,
        };
        if bytes.is_some() {
            return bytes;
        }
    }

    // For character input, use the `text` field which already accounts
    // for keyboard layout and modifiers (including Ctrl+letter producing
    // the correct control character).
    if let Some(t) = text {
        if !t.is_empty() {
            return Some(t.as_bytes().to_vec());
        }
    }

    None
}

/// I/O thread main loop.
///
/// Reads from the PTY master (non-blocking), parses VT sequences,
/// updates terminal state, and processes actions from the main thread.
fn run_io_thread(
    mut pty: Pty,
    terminal: Arc<Mutex<Terminal>>,
    io_rx: Receiver<IoAction>,
    main_tx: Sender<MainEvent>,
    proxy: EventLoopProxy<()>,
) {
    tracing::info!("I/O thread started");
    let mut parser = vte::Parser::new();
    let mut buf = [0u8; PTY_READ_BUF_SIZE];

    loop {
        // Process actions from the main thread (non-blocking drain)
        while let Ok(action) = io_rx.try_recv() {
            match action {
                IoAction::PtyWrite(data) => {
                    if let Err(e) = pty.write_all(&data) {
                        tracing::warn!("PTY write error: {e}");
                    }
                }
                IoAction::Resize { rows, cols } => {
                    if let Err(e) = pty.resize(rows, cols) {
                        tracing::warn!("PTY resize error: {e}");
                    }
                    if let Ok(mut term) = terminal.lock() {
                        term.resize(rows as usize, cols as usize);
                    }
                }
                IoAction::Shutdown => {
                    tracing::info!("I/O thread received shutdown");
                    let _ = pty.kill();
                    return;
                }
            }
        }

        // Try to read from PTY (non-blocking)
        match pty.read(&mut buf) {
            Ok(0) => {
                // EOF — child closed its end
                let code = pty.try_wait().ok().flatten().and_then(|s| s.code());
                let _ = main_tx.send(MainEvent::ChildExited(code));
                let _ = proxy.send_event(());
                return;
            }
            Ok(n) => {
                if let Ok(mut term) = terminal.lock() {
                    let mut handler = Handler::new(&mut term);
                    for &byte in &buf[..n] {
                        parser.advance(&mut handler, byte);
                    }
                }
                let _ = main_tx.send(MainEvent::Redraw);
                let _ = proxy.send_event(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available, sleep briefly to avoid busy-spinning
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(e) => {
                // Check if child has exited
                let code = pty.try_wait().ok().flatten().and_then(|s| s.code());
                if code.is_some() {
                    let _ = main_tx.send(MainEvent::ChildExited(code));
                } else {
                    tracing::warn!("PTY read error: {e}");
                    let _ = main_tx.send(MainEvent::ChildExited(None));
                }
                let _ = proxy.send_event(());
                return;
            }
        }
    }
}
