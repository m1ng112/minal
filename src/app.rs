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
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use base64::Engine;
use copypasta::{ClipboardContext, ClipboardProvider};

use minal_ai::CompletionEngine;
use minal_ai::provider::AiProvider;
use minal_config::KeybindAction;
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
    /// AI completion engine managing debounce.
    completion_engine: Option<CompletionEngine>,
    /// Current ghost text suggestion from AI.
    ghost_text: Option<String>,
    /// Current modifier state tracked from winit.
    modifiers: ModifiersState,
    /// Config file watcher for theme hot-reload.
    config_watcher: Option<crate::config_watcher::ConfigWatcher>,
    /// Mouse state tracking.
    mouse_state: crate::mouse::MouseState,
    /// System clipboard context (not Send, must stay on main thread).
    clipboard: Option<ClipboardContext>,
    /// Clipboard configuration (OSC 52 permissions, auto-copy).
    clipboard_config: minal_config::ClipboardConfig,
    /// Keybinding configuration for matching key events.
    keybind_config: minal_config::KeybindConfig,
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
            completion_engine: None,
            ghost_text: None,
            modifiers: ModifiersState::empty(),
            config_watcher: None,
            mouse_state: crate::mouse::MouseState::new(),
            clipboard: None,
            clipboard_config: minal_config::ClipboardConfig::default(),
            keybind_config: minal_config::KeybindConfig::default(),
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

    /// Clear the ghost text state and remove it from the terminal.
    fn clear_ghost_text(&mut self) {
        self.ghost_text = None;
        if let Some(ref terminal) = self.terminal {
            if let Ok(mut term) = terminal.lock() {
                term.clear_ghost_text();
            }
        }
    }

    /// Notify the completion engine of the current input line.
    fn notify_completion_engine(&mut self) {
        if let Some(ref mut engine) = self.completion_engine {
            if let Some(ref terminal) = self.terminal {
                if let Ok(term) = terminal.lock() {
                    let prefix = term.cursor_line_prefix();
                    engine.on_input_changed(&prefix);
                }
            }
        }
    }

    /// Check debounce and possibly trigger an AI completion request.
    fn check_debounce_and_request(&mut self) {
        let prefix = if let Some(ref mut engine) = self.completion_engine {
            engine.tick()
        } else {
            None
        };

        if let Some(prefix) = prefix {
            // Gather recent output context from terminal.
            let recent_output = if let Some(ref terminal) = self.terminal {
                if let Ok(term) = terminal.lock() {
                    let gatherer = minal_ai::ContextGatherer::default();
                    let ctx = gatherer.gather(&term);
                    ctx.recent_output
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            tracing::debug!(prefix = %prefix, "Requesting AI completion");
            self.send_io_event(IoEvent::AiComplete {
                prefix,
                recent_output,
            });
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

    /// Handle cursor moved event.
    fn handle_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.mouse_state.pixel_pos = (position.x, position.y);

        let (cell_width, cell_height, padding, max_cols, max_rows) = match self.get_cell_metrics() {
            Some(v) => v,
            None => return,
        };

        let (col, row) = crate::mouse::MouseState::pixel_to_cell(
            position.x,
            position.y,
            cell_width,
            cell_height,
            padding,
            max_cols,
            max_rows,
        );
        self.mouse_state.cell_pos = (col, row);

        if self.mouse_state.left_pressed {
            if let Some(ref terminal) = self.terminal {
                if let Ok(mut term) = terminal.lock() {
                    if term.mouse_tracking_active() {
                        // Send motion event to PTY if motion tracking is enabled.
                        if term.mouse_motion_tracking() {
                            let event = minal_core::mouse::MouseEvent {
                                kind: minal_core::mouse::MouseEventKind::Motion,
                                button: minal_core::mouse::MouseButton::Left,
                                col,
                                row,
                                modifiers: self.current_mouse_modifiers(),
                            };
                            let bytes = if term.sgr_mouse_mode() {
                                minal_core::mouse::encode_sgr(&event)
                            } else {
                                minal_core::mouse::encode_x10(&event)
                            };
                            drop(term);
                            self.send_io_event(IoEvent::Input(bytes));
                        }
                    } else {
                        // Update selection endpoint.
                        use minal_core::selection::SelectionPoint;
                        if let Some(mut sel) = term.selection().cloned() {
                            sel.update(SelectionPoint::new(row as i32, col));
                            term.set_selection(Some(sel));
                        }
                    }
                }
            }
            if let Some(ref w) = self.window {
                w.request_redraw();
            }
        }
    }

    /// Handle mouse button input event.
    fn handle_mouse_input(&mut self, state: ElementState, button: winit::event::MouseButton) {
        let (col, row) = self.mouse_state.cell_pos;

        match state {
            ElementState::Pressed => {
                // Clear ghost text on any mouse click.
                self.clear_ghost_text();

                let core_button = match button {
                    winit::event::MouseButton::Left => minal_core::mouse::MouseButton::Left,
                    winit::event::MouseButton::Middle => minal_core::mouse::MouseButton::Middle,
                    winit::event::MouseButton::Right => minal_core::mouse::MouseButton::Right,
                    _ => return,
                };

                if button == winit::event::MouseButton::Left {
                    self.mouse_state.left_pressed = true;
                }

                if let Some(ref terminal) = self.terminal {
                    if let Ok(mut term) = terminal.lock() {
                        if term.mouse_tracking_active() {
                            let event = minal_core::mouse::MouseEvent {
                                kind: minal_core::mouse::MouseEventKind::Press,
                                button: core_button,
                                col,
                                row,
                                modifiers: self.current_mouse_modifiers(),
                            };
                            let bytes = if term.sgr_mouse_mode() {
                                minal_core::mouse::encode_sgr(&event)
                            } else {
                                minal_core::mouse::encode_x10(&event)
                            };
                            drop(term);
                            self.send_io_event(IoEvent::Input(bytes));
                        } else if button == winit::event::MouseButton::Left {
                            // Handle selection.
                            let click_count = self.mouse_state.register_click(col, row);
                            use minal_core::selection::{
                                Selection, SelectionPoint, SelectionType, word_end, word_start,
                            };

                            match click_count {
                                2 => {
                                    // Double-click: word selection.
                                    let ws = word_start(term.grid(), row, col);
                                    let we = word_end(term.grid(), row, col);
                                    let mut sel = Selection::new(
                                        SelectionType::Simple,
                                        SelectionPoint::new(row as i32, ws),
                                    );
                                    sel.update(SelectionPoint::new(row as i32, we));
                                    term.set_selection(Some(sel));
                                }
                                3 => {
                                    // Triple-click: line selection.
                                    let mut sel = Selection::new(
                                        SelectionType::Lines,
                                        SelectionPoint::new(row as i32, 0),
                                    );
                                    sel.update(SelectionPoint::new(
                                        row as i32,
                                        term.cols().saturating_sub(1),
                                    ));
                                    term.set_selection(Some(sel));
                                }
                                _ => {
                                    // Single click: start new selection.
                                    let sel = Selection::new(
                                        SelectionType::Simple,
                                        SelectionPoint::new(row as i32, col),
                                    );
                                    term.set_selection(Some(sel));
                                }
                            }
                        }
                    }
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            ElementState::Released => {
                if button == winit::event::MouseButton::Left {
                    self.mouse_state.left_pressed = false;
                }

                let core_button = match button {
                    winit::event::MouseButton::Left => minal_core::mouse::MouseButton::Left,
                    winit::event::MouseButton::Middle => minal_core::mouse::MouseButton::Middle,
                    winit::event::MouseButton::Right => minal_core::mouse::MouseButton::Right,
                    _ => return,
                };

                let mut tracking_active = false;
                if let Some(ref terminal) = self.terminal {
                    if let Ok(term) = terminal.lock() {
                        tracking_active = term.mouse_tracking_active();
                        if tracking_active {
                            let event = minal_core::mouse::MouseEvent {
                                kind: minal_core::mouse::MouseEventKind::Release,
                                button: core_button,
                                col,
                                row,
                                modifiers: self.current_mouse_modifiers(),
                            };
                            let bytes = if term.sgr_mouse_mode() {
                                minal_core::mouse::encode_sgr(&event)
                            } else {
                                minal_core::mouse::encode_x10(&event)
                            };
                            drop(term);
                            self.send_io_event(IoEvent::Input(bytes));
                        }
                    }
                }

                // Auto-copy on selection release (when not in mouse tracking mode).
                if button == winit::event::MouseButton::Left
                    && !tracking_active
                    && self.clipboard_config.auto_copy_on_select
                    && self.clipboard_copy()
                {
                    tracing::debug!("Auto-copied selection to clipboard");
                }
            }
        }
    }

    /// Handle mouse wheel event.
    fn handle_mouse_wheel(&mut self, delta: winit::event::MouseScrollDelta) {
        let (col, row) = self.mouse_state.cell_pos;

        let lines = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => y as i32,
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                let cell_height = self.renderer.as_ref().map_or(20.0, |r| r.cell_size().1);
                (pos.y as f32 / cell_height) as i32
            }
        };

        if lines == 0 {
            return;
        }

        if let Some(ref terminal) = self.terminal {
            if let Ok(mut term) = terminal.lock() {
                if term.mouse_tracking_active() {
                    // Send wheel events as mouse button presses.
                    let button = if lines > 0 {
                        minal_core::mouse::MouseButton::WheelUp
                    } else {
                        minal_core::mouse::MouseButton::WheelDown
                    };
                    let count = lines.unsigned_abs() as usize;
                    let modifiers = self.current_mouse_modifiers();

                    for _ in 0..count {
                        let event = minal_core::mouse::MouseEvent {
                            kind: minal_core::mouse::MouseEventKind::Press,
                            button,
                            col,
                            row,
                            modifiers,
                        };
                        let bytes = if term.sgr_mouse_mode() {
                            minal_core::mouse::encode_sgr(&event)
                        } else {
                            minal_core::mouse::encode_x10(&event)
                        };
                        self.send_io_event(IoEvent::Input(bytes));
                    }
                } else {
                    // Scrollback navigation.
                    let count = lines.unsigned_abs() as usize;
                    if lines > 0 {
                        term.scroll_display_up(count);
                    } else {
                        term.scroll_display_down(count);
                    }
                }
            }
        }

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    /// Get current cell metrics for mouse coordinate conversion.
    fn get_cell_metrics(&self) -> Option<(f32, f32, f32, usize, usize)> {
        let renderer = self.renderer.as_ref()?;
        let terminal = self.terminal.as_ref()?;
        let (cell_width, cell_height) = renderer.cell_size();
        let padding = renderer.padding();
        let term = terminal.lock().ok()?;
        let max_cols = term.cols();
        let max_rows = term.rows();
        Some((cell_width, cell_height, padding, max_cols, max_rows))
    }

    /// Get current mouse modifier state from winit modifiers.
    fn current_mouse_modifiers(&self) -> minal_core::mouse::MouseModifiers {
        minal_core::mouse::MouseModifiers {
            shift: self.modifiers.shift_key(),
            alt: self.modifiers.alt_key(),
            ctrl: self.modifiers.control_key(),
        }
    }

    /// Check if a key event matches a configured keybinding with the given action.
    fn matches_keybind_action(
        &self,
        key_event: &winit::event::KeyEvent,
        action: &KeybindAction,
    ) -> bool {
        for binding in &self.keybind_config.bindings {
            if &binding.action != action {
                continue;
            }
            // Check key match
            let key_matches = match &key_event.logical_key {
                Key::Character(c) => c.as_str().eq_ignore_ascii_case(&binding.key),
                Key::Named(named) => {
                    // Compare without heap allocation using Debug trait on stack buffer.
                    let mut buf = [0u8; 32];
                    let mut cursor = std::io::Cursor::new(&mut buf[..]);
                    let name = if std::io::Write::write_fmt(&mut cursor, format_args!("{named:?}"))
                        .is_ok()
                    {
                        let pos = cursor.position() as usize;
                        std::str::from_utf8(&buf[..pos]).ok()
                    } else {
                        None
                    };
                    name.is_some_and(|n| n.eq_ignore_ascii_case(&binding.key))
                }
                _ => false,
            };
            if !key_matches {
                continue;
            }
            // Check modifiers
            let mods_match = binding.modifiers.iter().all(|m| match m.as_str() {
                "Super" => self.modifiers.super_key(),
                "Ctrl" | "Control" => self.modifiers.control_key(),
                "Shift" => self.modifiers.shift_key(),
                "Alt" | "Option" => self.modifiers.alt_key(),
                _ => false,
            });
            // Also check that no extra modifiers are pressed beyond those required.
            let required_super = binding.modifiers.iter().any(|m| m == "Super");
            let required_ctrl = binding
                .modifiers
                .iter()
                .any(|m| m == "Ctrl" || m == "Control");
            let required_shift = binding.modifiers.iter().any(|m| m == "Shift");
            let required_alt = binding
                .modifiers
                .iter()
                .any(|m| m == "Alt" || m == "Option");
            let extra_mods = (self.modifiers.super_key() != required_super)
                || (self.modifiers.control_key() != required_ctrl)
                || (self.modifiers.shift_key() != required_shift)
                || (self.modifiers.alt_key() != required_alt);

            if mods_match && !extra_mods {
                return true;
            }
        }
        false
    }

    /// Try to copy selected text to the system clipboard. Returns true if text was copied.
    fn clipboard_copy(&mut self) -> bool {
        let text = if let Some(ref terminal) = self.terminal {
            if let Ok(term) = terminal.lock() {
                term.selected_text()
            } else {
                None
            }
        } else {
            None
        };
        if let Some(text) = text {
            if let Some(ref mut ctx) = self.clipboard {
                if let Err(e) = ctx.set_contents(text) {
                    tracing::warn!("Failed to set clipboard contents: {e}");
                    return false;
                }
                return true;
            }
        }
        false
    }

    /// Paste clipboard contents into the PTY, respecting bracketed paste mode.
    fn clipboard_paste(&mut self) {
        let text = if let Some(ref mut ctx) = self.clipboard {
            match ctx.get_contents() {
                Ok(t) => Some(t),
                Err(e) => {
                    tracing::warn!("Failed to get clipboard contents: {e}");
                    None
                }
            }
        } else {
            None
        };
        if let Some(text) = text {
            if text.is_empty() {
                return;
            }
            let bracketed = self
                .terminal
                .as_ref()
                .and_then(|t| t.lock().ok())
                .is_some_and(|t| t.mode(Mode::BracketedPaste));
            let mut data = Vec::new();
            if bracketed {
                data.extend_from_slice(b"\x1b[200~");
            }
            data.extend_from_slice(text.as_bytes());
            if bracketed {
                data.extend_from_slice(b"\x1b[201~");
            }
            self.send_io_event(IoEvent::Input(data));
            self.clear_ghost_text();
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
        let ai_config = config.ai.clone();
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
                rt.block_on(io_loop(pty, io_rx, terminal_clone, proxy_clone, ai_config));
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

        // Initialize AI completion engine.
        if config.ai.enabled {
            let engine = CompletionEngine::new(config.ai.debounce_ms);
            self.completion_engine = Some(engine);
            tracing::info!(
                "AI completion enabled (debounce: {}ms)",
                config.ai.debounce_ms
            );
        }

        // Initialize clipboard context.
        match ClipboardContext::new() {
            Ok(ctx) => {
                self.clipboard = Some(ctx);
                tracing::info!("Clipboard support initialized");
            }
            Err(e) => {
                tracing::warn!("Failed to initialize clipboard: {e}");
                self.clipboard = None;
            }
        }
        self.clipboard_config = config.clipboard.clone();
        self.keybind_config = config.keybinds.clone();

        // Start config file watcher for theme hot-reload.
        self.config_watcher = match minal_config::Config::config_path() {
            Ok(path) => match crate::config_watcher::ConfigWatcher::new(path, self.proxy.clone()) {
                Ok(w) => {
                    tracing::info!("Config file watcher started");
                    Some(w)
                }
                Err(e) => {
                    tracing::warn!("Failed to start config watcher: {e}");
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to determine config path for watcher: {e}");
                None
            }
        };

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
            WakeupReason::CompletionReady(text) => {
                if text.is_empty() {
                    tracing::debug!("AI returned empty completion");
                    return;
                }
                tracing::debug!(completion = %text, "AI completion received");
                self.ghost_text = Some(text.clone());
                if let Some(ref terminal) = self.terminal {
                    if let Ok(mut term) = terminal.lock() {
                        term.set_ghost_text(text);
                    }
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::CompletionFailed => {
                tracing::debug!("AI completion request failed");
                self.clear_ghost_text();
            }
            WakeupReason::ThemeChanged(ref theme) => {
                if let Some(ref mut renderer) = self.renderer {
                    renderer.update_theme(theme);
                }
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
                tracing::info!("Theme hot-reloaded");
            }
            WakeupReason::ClipboardSet(text) => {
                if self.clipboard_config.osc52_write {
                    if let Some(ref mut ctx) = self.clipboard {
                        if let Err(e) = ctx.set_contents(text) {
                            tracing::warn!("OSC 52: failed to set clipboard: {e}");
                        } else {
                            tracing::debug!("OSC 52: clipboard set");
                        }
                    }
                } else {
                    tracing::debug!("OSC 52 write blocked by configuration");
                }
            }
            WakeupReason::ClipboardGet => {
                if self.clipboard_config.osc52_read {
                    if let Some(ref mut ctx) = self.clipboard {
                        match ctx.get_contents() {
                            Ok(text) => {
                                let engine = base64::engine::general_purpose::STANDARD;
                                let encoded = engine.encode(text.as_bytes());
                                let response = format!("\x1b]52;c;{encoded}\x07");
                                self.send_io_event(IoEvent::Input(response.into_bytes()));
                            }
                            Err(e) => {
                                tracing::warn!("OSC 52: failed to get clipboard: {e}");
                            }
                        }
                    }
                } else {
                    tracing::debug!("OSC 52 read blocked by configuration");
                }
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
        // Track modifier state from winit.
        if let WindowEvent::ModifiersChanged(mods) = &event {
            self.modifiers = mods.state();
        }

        // Handle events that need full &mut self access before borrowing
        // gpu/renderer fields.
        match &event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                self.shutdown();
                event_loop.exit();
                return;
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state != ElementState::Pressed {
                    return;
                }

                let has_ctrl = self.modifiers.control_key();
                let has_shift = self.modifiers.shift_key();

                // Ctrl+Shift+A: Toggle AI completion.
                if has_ctrl
                    && has_shift
                    && matches!(
                        key_event.logical_key,
                        Key::Character(ref s) if s.as_str().eq_ignore_ascii_case("a")
                    )
                {
                    if let Some(ref mut engine) = self.completion_engine {
                        engine.toggle();
                        let enabled = engine.is_enabled();
                        tracing::info!(
                            "AI completion toggled: {}",
                            if enabled { "on" } else { "off" }
                        );
                        if !enabled {
                            self.clear_ghost_text();
                        }
                    }
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }

                // Copy keybinding (Cmd+C on macOS, Ctrl+Shift+C on Linux).
                if self.matches_keybind_action(key_event, &KeybindAction::Copy) {
                    if self.clipboard_copy() {
                        tracing::debug!("Copied selection to clipboard");
                    }
                    return;
                }

                // Paste keybinding (Cmd+V on macOS, Ctrl+Shift+V on Linux).
                if self.matches_keybind_action(key_event, &KeybindAction::Paste) {
                    self.clipboard_paste();
                    return;
                }

                // Tab when ghost text is active: accept completion.
                if self.ghost_text.is_some()
                    && matches!(key_event.logical_key, Key::Named(NamedKey::Tab))
                {
                    if let Some(text) = self.ghost_text.take() {
                        tracing::debug!(accepted = %text, "AI completion accepted");
                        self.send_io_event(IoEvent::Input(text.into_bytes()));
                        self.clear_ghost_text();
                        if let Some(ref mut engine) = self.completion_engine {
                            engine.clear();
                        }
                    }
                    self.cursor_visible = true;
                    self.last_blink = Instant::now();
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }

                // Escape when ghost text is active: dismiss completion.
                if self.ghost_text.is_some()
                    && matches!(key_event.logical_key, Key::Named(NamedKey::Escape))
                {
                    tracing::debug!("AI completion dismissed");
                    self.clear_ghost_text();
                    if let Some(ref mut engine) = self.completion_engine {
                        engine.clear();
                    }
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }

                // Normal key input.
                if let Some(bytes) = self.translate_key_input(key_event) {
                    self.send_io_event(IoEvent::Input(bytes));

                    // Clear selection on keyboard input.
                    if let Some(ref terminal) = self.terminal {
                        if let Ok(mut term) = terminal.lock() {
                            term.set_selection(None);
                        }
                    }

                    // New input invalidates old ghost text.
                    self.clear_ghost_text();

                    // Notify the completion engine about input change.
                    self.notify_completion_engine();

                    // Reset cursor blink to visible on input.
                    self.cursor_visible = true;
                    self.last_blink = Instant::now();
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                }
                return;
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(*position);
                return;
            }

            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(*state, *button);
                return;
            }

            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(*delta);
                return;
            }

            WindowEvent::RedrawRequested => {
                // Check if debounce has elapsed and trigger AI completion
                // before borrowing gpu/renderer.
                self.check_debounce_and_request();
            }

            _ => {}
        }

        let (Some(window), Some(gpu), Some(renderer)) = (
            self.window.as_ref(),
            self.gpu.as_mut(),
            self.renderer.as_mut(),
        ) else {
            return;
        };

        match event {
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

                let ghost = term.ghost_text();
                let selection = term.selection();
                renderer.render(
                    gpu.device(),
                    gpu.queue(),
                    &frame.view,
                    w,
                    h,
                    term.grid(),
                    &cursor,
                    ghost,
                    selection,
                );

                term.clear_dirty();
                // Drop the lock before present to minimize lock hold time.
                drop(term);

                frame.present();

                // Schedule the next wakeup: consider both blink and debounce deadlines.
                let next_blink = self.last_blink + blink_interval;
                let mut next_wakeup = next_blink;

                if let Some(ref engine) = self.completion_engine {
                    if let Some(debounce_deadline) = engine.debounce_deadline() {
                        if debounce_deadline < next_wakeup {
                            next_wakeup = debounce_deadline;
                        }
                    }
                }

                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_wakeup));
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
    ai_config: minal_config::AiConfig,
) {
    let async_pty = match AsyncPty::from_pty(pty) {
        Ok(ap) => ap,
        Err(e) => {
            tracing::error!("Failed to create AsyncPty: {e}");
            let _ = proxy.send_event(WakeupReason::ChildExited(1));
            return;
        }
    };

    // Create AI provider if enabled.
    // Phase 1 MVP: only Ollama is supported. Other providers are Phase 3 scope.
    let ai_provider: Option<Arc<dyn AiProvider>> = if ai_config.enabled {
        match minal_ai::OllamaProvider::new(ai_config.base_url.clone(), ai_config.model.clone()) {
            Ok(provider) => {
                tracing::debug!("Ollama AI provider created for I/O thread");
                Some(Arc::new(provider))
            }
            Err(e) => {
                tracing::warn!("Failed to create Ollama provider: {e}");
                None
            }
        }
    } else {
        None
    };
    let mut ai_task: Option<tokio::task::JoinHandle<()>> = None;

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
                            // Check for pending clipboard actions from OSC 52.
                            for clipboard_action in term.take_pending_clipboard() {
                                match clipboard_action {
                                    minal_core::term::ClipboardAction::SetContents(text) => {
                                        let _ = proxy.send_event(
                                            WakeupReason::ClipboardSet(text),
                                        );
                                    }
                                    minal_core::term::ClipboardAction::RequestContents => {
                                        let _ = proxy.send_event(WakeupReason::ClipboardGet);
                                    }
                                }
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
                    Some(IoEvent::AiComplete { prefix, recent_output }) => {
                        if let Some(ref provider) = ai_provider {
                            // Cancel any in-flight completion task.
                            if let Some(task) = ai_task.take() {
                                task.abort();
                            }
                            let provider = Arc::clone(provider);
                            let proxy_clone = proxy.clone();
                            let context = minal_ai::CompletionContext {
                                cwd: None,
                                input_prefix: prefix,
                                recent_output,
                            };
                            // Spawn so PTY reads are not blocked during
                            // the HTTP request.
                            ai_task = Some(tokio::spawn(async move {
                                match provider.complete(&context).await {
                                    Ok(completion) if !completion.is_empty() => {
                                        let _ = proxy_clone.send_event(
                                            WakeupReason::CompletionReady(completion),
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(e) => {
                                        tracing::debug!("AI completion error: {e}");
                                        let _ = proxy_clone.send_event(
                                            WakeupReason::CompletionFailed,
                                        );
                                    }
                                }
                            }));
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
