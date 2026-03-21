//! Main application event loop with tab and pane support.
//!
//! Integrates the 3-thread architecture:
//! - **Main thread**: winit event loop + wgpu rendering
//! - **I/O threads**: one per pane, tokio runtime for async PTY read/write + VT parsing
//!
//! Communication:
//! - Main -> I/O: crossbeam-channel per-pane [`Sender<IoEvent>`]
//! - I/O -> Main: shared winit [`EventLoopProxy<WakeupReason>`]
//! - Shared state: per-pane [`Arc<Mutex<Terminal>>`]

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use base64::Engine;
use copypasta::{ClipboardContext, ClipboardProvider};

use minal_config::KeybindAction;
use minal_core::ansi::Mode;
use minal_core::pty::PtySize;
use minal_renderer::renderer::TAB_BAR_HEIGHT;
use minal_renderer::{GpuContext, Renderer, RendererError, TabBarInfo, Viewport};

use crate::event::{IoEvent, WakeupReason};
use crate::pane::PaneId;
use crate::tab::{Rect, SplitDirection, TabManager};

/// Default window width in logical pixels.
const DEFAULT_WIDTH: u32 = 800;
/// Default window height in logical pixels.
const DEFAULT_HEIGHT: u32 = 600;
/// Window title.
const WINDOW_TITLE: &str = "Minal";

/// Cursor blink interval in milliseconds.
const CURSOR_BLINK_MS: u64 = 600;

/// macOS titlebar height in logical pixels (standard, non-notched displays).
#[cfg(target_os = "macos")]
const MACOS_TITLEBAR_HEIGHT: f32 = 28.0;

/// Build environment variables for shell integration.
///
/// Sets `TERM_PROGRAM=minal` and `MINAL_SHELL_INTEGRATION` pointing to the
/// shell integration scripts directory (relative to the executable).
fn shell_integration_env_vars() -> Vec<(String, String)> {
    let mut vars = vec![
        ("TERM_PROGRAM".to_string(), "minal".to_string()),
        (
            "TERM_PROGRAM_VERSION".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ),
    ];

    // Respect an existing MINAL_SHELL_INTEGRATION value (power-user override).
    if let Ok(existing) = std::env::var("MINAL_SHELL_INTEGRATION") {
        vars.push(("MINAL_SHELL_INTEGRATION".to_string(), existing));
        return vars;
    }

    // Try to locate shell-integration/ relative to the executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // In development: try ../../shell-integration (from target/debug/)
            // In production: try ../share/minal/shell-integration or ./shell-integration
            let candidates = [
                dir.join("shell-integration"),
                dir.join("../share/minal/shell-integration"),
                dir.join("../../shell-integration"),
            ];
            for candidate in &candidates {
                if candidate.is_dir() {
                    if let Some(path) = candidate
                        .canonicalize()
                        .ok()
                        .and_then(|p| p.to_str().map(String::from))
                    {
                        vars.push(("MINAL_SHELL_INTEGRATION".to_string(), path));
                        break;
                    }
                }
            }
        }
    }

    vars
}

/// Divider drag state.
struct DividerDrag {
    node_path: u64,
    direction: SplitDirection,
}

/// Main application state implementing winit's `ApplicationHandler`.
pub struct App {
    proxy: EventLoopProxy<WakeupReason>,
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    renderer: Option<Renderer>,
    tab_manager: Option<TabManager>,
    /// Whether the cursor blink is currently in the visible phase.
    cursor_visible: bool,
    /// Timestamp of the last cursor blink toggle.
    last_blink: Instant,
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
    /// Stored config for spawning new panes.
    config: Option<minal_config::Config>,
    /// Current divider drag state.
    divider_drag: Option<DividerDrag>,
    /// Active IME preedit state: `(text, cursor_range)`.
    ///
    /// Set while the input method is composing a character sequence.
    /// The normal key-input path is suppressed while this is `Some`.
    ime_preedit: Option<(String, Option<(usize, usize)>)>,
    /// Inline AI chat panel state.
    chat_panel: Option<crate::chat::ChatPanelState>,
    /// Error analysis panel state.
    error_panel: Option<crate::error_panel_state::ErrorPanelState>,
    /// Timestamp of the last frame for animation delta time.
    last_frame_time: Instant,
}

impl App {
    /// Creates a new uninitialized application with the given event loop proxy.
    pub fn new(proxy: EventLoopProxy<WakeupReason>) -> Self {
        Self {
            proxy,
            window: None,
            gpu: None,
            renderer: None,
            tab_manager: None,
            cursor_visible: true,
            last_blink: Instant::now(),
            modifiers: ModifiersState::empty(),
            config_watcher: None,
            mouse_state: crate::mouse::MouseState::new(),
            clipboard: None,
            clipboard_config: minal_config::ClipboardConfig::default(),
            keybind_config: minal_config::KeybindConfig::default(),
            config: None,
            divider_drag: None,
            ime_preedit: None,
            chat_panel: None,
            error_panel: None,
            last_frame_time: Instant::now(),
        }
    }

    /// Compute terminal grid dimensions from a viewport size and cell metrics.
    fn compute_grid_size(
        width: f32,
        height: f32,
        cell_width: f32,
        cell_height: f32,
        padding: f32,
    ) -> (usize, usize) {
        let usable_width = (width - padding * 2.0).max(0.0);
        let usable_height = (height - padding * 2.0).max(0.0);
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

    /// Compute the content viewport (area below the tab bar).
    ///
    /// On macOS with `fullsize_content_view` enabled the window content extends
    /// behind the transparent titlebar.  We reserve the top 28 px so that the
    /// traffic-light buttons do not overlap the first row of terminal output.
    fn content_viewport(&self) -> Rect {
        let (w, h) = self.gpu.as_ref().map_or((800, 600), |g| g.size());
        let show_tab_bar = self
            .tab_manager
            .as_ref()
            .is_some_and(|tm| tm.tab_count() > 1);
        let tab_bar_h = if show_tab_bar { TAB_BAR_HEIGHT } else { 0.0 };

        // On macOS we use a full-size content view with a transparent titlebar.
        // Reserve space for the title-bar (traffic lights) so terminal content
        // is not obscured.
        #[cfg(target_os = "macos")]
        let titlebar_inset = MACOS_TITLEBAR_HEIGHT;
        #[cfg(not(target_os = "macos"))]
        let titlebar_inset: f32 = 0.0;

        let top_offset = tab_bar_h + titlebar_inset;
        Rect {
            x: 0.0,
            y: top_offset,
            width: w as f32,
            height: (h as f32 - top_offset).max(0.0),
        }
    }

    /// Spawn a new pane with the stored configuration.
    fn spawn_pane(&mut self, rows: usize, cols: usize) -> Option<crate::pane::Pane> {
        let config = self.config.as_ref()?;
        let tab_manager = self.tab_manager.as_mut()?;
        let pane_id = tab_manager.next_pane_id();
        let shell = config.shell.resolve_program();
        let env_vars = shell_integration_env_vars();

        match crate::pane::Pane::spawn(
            pane_id,
            rows,
            cols,
            &shell,
            self.proxy.clone(),
            &config.ai,
            &env_vars,
        ) {
            Ok(pane) => Some(pane),
            Err(e) => {
                tracing::error!("Failed to spawn pane: {e}");
                None
            }
        }
    }

    /// Get the focused pane's terminal, io_tx, etc. for operations.
    fn with_focused_pane<F, R>(&mut self, f: F) -> Option<R>
    where
        F: FnOnce(&mut crate::pane::Pane) -> R,
    {
        let tab_manager = self.tab_manager.as_mut()?;
        let tab = tab_manager.active_tab_mut()?;
        let pane = tab.focused_pane_mut()?;
        Some(f(pane))
    }

    /// Send an I/O event to the focused pane.
    fn send_to_focused(&self, event: IoEvent) {
        if let Some(ref tm) = self.tab_manager {
            if let Some(tab) = tm.active_tab() {
                if let Some(pane) = tab.focused_pane() {
                    pane.send_io_event(event);
                }
            }
        }
    }

    /// Clear ghost text on the focused pane.
    fn clear_focused_ghost_text(&mut self) {
        self.with_focused_pane(|pane| pane.clear_ghost_text());
    }

    /// Check if the focused pane has ghost text.
    fn focused_has_ghost_text(&self) -> bool {
        self.tab_manager
            .as_ref()
            .and_then(|tm| tm.active_tab())
            .and_then(|tab| tab.focused_pane())
            .and_then(|pane| pane.ghost_text.as_ref())
            .is_some()
    }

    /// Check debounce on the focused pane and maybe trigger AI completion.
    fn check_focused_debounce(&mut self) {
        self.with_focused_pane(|pane| pane.check_debounce_and_request());
    }

    /// Handle keyboard input when the chat panel is focused.
    fn handle_chat_key_input(&mut self, key_event: &winit::event::KeyEvent) {
        let panel = match self.chat_panel.as_mut() {
            Some(p) => p,
            None => return,
        };

        match &key_event.logical_key {
            Key::Named(named) => match named {
                NamedKey::Escape => {
                    panel.toggle();
                    tracing::info!("Chat panel closed via Escape");
                }
                NamedKey::Enter => {
                    // Shift+Enter inserts a newline.
                    if self.modifiers.shift_key() {
                        panel.insert_char('\n');
                    } else {
                        self.send_chat_message();
                    }
                }
                NamedKey::Backspace => {
                    panel.backspace();
                }
                NamedKey::Delete => {
                    panel.delete_char();
                }
                NamedKey::ArrowLeft => {
                    panel.cursor_left();
                }
                NamedKey::ArrowRight => {
                    panel.cursor_right();
                }
                NamedKey::ArrowUp => {
                    panel.scroll_up(20.0);
                }
                NamedKey::ArrowDown => {
                    panel.scroll_down(20.0);
                }
                NamedKey::Home => {
                    panel.cursor_home();
                }
                NamedKey::End => {
                    panel.cursor_end();
                }
                _ => {}
            },
            Key::Character(text) => {
                let s = text.as_str();
                // Don't insert control characters.
                if !self.modifiers.control_key() && !self.modifiers.super_key() {
                    for ch in s.chars() {
                        panel.insert_char(ch);
                    }
                }
            }
            _ => {}
        }

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    /// Send the current chat input as a message to the AI provider.
    fn send_chat_message(&mut self) {
        let panel = match self.chat_panel.as_mut() {
            Some(p) => p,
            None => return,
        };

        let text = match panel.take_input() {
            Some(t) => t,
            None => return,
        };

        let messages = panel.chat_engine.add_user_message(&text);

        // Gather terminal context from the focused pane.
        let context = self.with_focused_pane(|pane| {
            pane.context_collector.as_mut().and_then(|collector| {
                pane.terminal
                    .lock()
                    .ok()
                    .map(|term| collector.gather(&term))
            })
        });

        let context = context.flatten().unwrap_or_else(|| minal_ai::AiContext {
            cwd: None,
            input_prefix: String::new(),
            recent_output: Vec::new(),
            shell: None,
            os: None,
            git_branch: None,
            git_info: None,
            project_type: None,
            command_history: Vec::new(),
            env_hints: Vec::new(),
        });

        self.send_to_focused(IoEvent::AiChat { messages, context });
        tracing::debug!("Chat message sent to AI provider");
    }

    /// Translate a keyboard event to bytes to send to the PTY.
    fn translate_key_input(&self, event: &winit::event::KeyEvent) -> Option<Vec<u8>> {
        if event.state != ElementState::Pressed {
            return None;
        }

        // Check if the focused pane's terminal is in application cursor key mode.
        let app_cursor = self
            .tab_manager
            .as_ref()
            .and_then(|tm| tm.active_tab())
            .and_then(|tab| tab.focused_pane())
            .and_then(|pane| pane.terminal.lock().ok())
            .is_some_and(|t| t.mode(Mode::CursorKeys));

        match &event.logical_key {
            Key::Named(named) => {
                let bytes = match named {
                    NamedKey::Enter => b"\r".to_vec(),
                    NamedKey::Backspace => vec![0x7f],
                    NamedKey::Tab => b"\t".to_vec(),
                    NamedKey::Escape => vec![0x1b],
                    NamedKey::ArrowUp if app_cursor => b"\x1bOA".to_vec(),
                    NamedKey::ArrowUp => b"\x1b[A".to_vec(),
                    NamedKey::ArrowDown if app_cursor => b"\x1bOB".to_vec(),
                    NamedKey::ArrowDown => b"\x1b[B".to_vec(),
                    NamedKey::ArrowRight if app_cursor => b"\x1bOC".to_vec(),
                    NamedKey::ArrowRight => b"\x1b[C".to_vec(),
                    NamedKey::ArrowLeft if app_cursor => b"\x1bOD".to_vec(),
                    NamedKey::ArrowLeft => b"\x1b[D".to_vec(),
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
                let s = text.as_str();
                if s.is_empty() {
                    return None;
                }
                Some(s.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    /// Handle cursor moved event with pane-aware coordinate mapping.
    fn handle_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.mouse_state.pixel_pos = (position.x, position.y);

        // Handle divider drag.
        if let Some(ref drag) = self.divider_drag {
            let viewport = self.content_viewport();
            let new_ratio = match drag.direction {
                SplitDirection::Vertical => {
                    ((position.x as f32 - viewport.x) / viewport.width).clamp(0.1, 0.9)
                }
                SplitDirection::Horizontal => {
                    ((position.y as f32 - viewport.y) / viewport.height).clamp(0.1, 0.9)
                }
            };
            let path = drag.node_path;
            if let Some(ref mut tm) = self.tab_manager {
                if let Some(tab) = tm.active_tab_mut() {
                    tab.root.set_divider_ratio_at_path(path, new_ratio);
                }
            }
            // Resize panes after divider drag.
            self.resize_all_panes();
            if let Some(ref w) = self.window {
                w.request_redraw();
            }
            return;
        }

        let renderer = match self.renderer.as_ref() {
            Some(r) => r,
            None => return,
        };
        let (cell_width, cell_height) = renderer.cell_size();
        let padding = renderer.padding();

        // Determine which pane the cursor is over and compute pane-relative coords.
        let viewport = self.content_viewport();
        let (col, row, _pane_id) = if let Some(ref tm) = self.tab_manager {
            if let Some(tab) = tm.active_tab() {
                let layouts = tab.layout(viewport);
                let mut found = None;
                let px = position.x as f32;
                let py = position.y as f32;
                for (pid, rect) in &layouts {
                    let in_bounds = px >= rect.x
                        && px <= rect.x + rect.width
                        && py >= rect.y
                        && py <= rect.y + rect.height;
                    if in_bounds {
                        // Compute pane-relative cell coords using the
                        // pane under the cursor (not just the focused one).
                        let (max_cols, max_rows) = tab
                            .root
                            .find_pane(*pid)
                            .and_then(|p| p.terminal.lock().ok())
                            .map_or((80, 24), |t| (t.cols(), t.rows()));
                        let (col, row) = crate::mouse::MouseState::pixel_to_cell(
                            position.x - rect.x as f64,
                            position.y - rect.y as f64,
                            cell_width,
                            cell_height,
                            padding,
                            max_cols,
                            max_rows,
                        );
                        found = Some((col, row, *pid));
                        break;
                    }
                }
                found.unwrap_or((0, 0, PaneId(0)))
            } else {
                (0, 0, PaneId(0))
            }
        } else {
            (0, 0, PaneId(0))
        };

        self.mouse_state.cell_pos = (col, row);

        if self.mouse_state.left_pressed {
            if let Some(ref tm) = self.tab_manager {
                if let Some(tab) = tm.active_tab() {
                    if let Some(pane) = tab.focused_pane() {
                        if let Ok(mut term) = pane.terminal.lock() {
                            if term.mouse_tracking_active() {
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
                                    pane.send_io_event(IoEvent::Input(bytes));
                                }
                            } else {
                                use minal_core::selection::SelectionPoint;
                                if let Some(mut sel) = term.selection().cloned() {
                                    sel.update(SelectionPoint::new(row as i32, col));
                                    term.set_selection(Some(sel));
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ref w) = self.window {
                w.request_redraw();
            }
        }
    }

    /// Handle mouse button input event with pane-aware dispatch.
    fn handle_mouse_input(&mut self, state: ElementState, button: winit::event::MouseButton) {
        let (col, row) = self.mouse_state.cell_pos;

        match state {
            ElementState::Pressed => {
                // Check if the click is inside the chat panel.
                if button == winit::event::MouseButton::Left {
                    if let Some(ref mut panel) = self.chat_panel {
                        if panel.is_visible() && !panel.is_fully_hidden() {
                            let px = self.mouse_state.pixel_pos.0 as f32;
                            let py = self.mouse_state.pixel_pos.1 as f32;
                            let (sw, sh) = self.gpu.as_ref().map_or((800, 600), |g| g.size());
                            let show_tab_bar = self
                                .tab_manager
                                .as_ref()
                                .is_some_and(|tm| tm.tab_count() > 1);
                            let tab_bar_h = if show_tab_bar { TAB_BAR_HEIGHT } else { 0.0 };
                            #[cfg(target_os = "macos")]
                            let titlebar_inset = MACOS_TITLEBAR_HEIGHT;
                            #[cfg(not(target_os = "macos"))]
                            let titlebar_inset: f32 = 0.0;
                            let top_offset = tab_bar_h + titlebar_inset;
                            let vp = panel.panel_viewport(sw as f32, sh as f32, top_offset);

                            // Check if click is within the panel bounds.
                            if py >= vp.y
                                && py <= vp.y + vp.height
                                && px >= vp.x
                                && px <= vp.x + vp.width
                            {
                                // Check hit regions for code block execute buttons.
                                for region in &panel.hit_regions {
                                    match region {
                                        minal_renderer::ChatHitRegion::ExecuteCodeBlock {
                                            index,
                                            code,
                                            rect,
                                        } => {
                                            if px >= rect.x
                                                && px <= rect.x + rect.width
                                                && py >= rect.y
                                                && py <= rect.y + rect.height
                                            {
                                                let code = code.clone();
                                                tracing::info!(
                                                    block_index = index,
                                                    "Pasting code block from chat (user must confirm with Enter)"
                                                );
                                                // Paste code into the terminal input
                                                // WITHOUT sending Enter, so the user
                                                // can review before executing.
                                                self.send_to_focused(IoEvent::Input(
                                                    code.into_bytes(),
                                                ));
                                                if let Some(ref w) = self.window {
                                                    w.request_redraw();
                                                }
                                                return;
                                            }
                                        }
                                        minal_renderer::ChatHitRegion::CloseButton { rect } => {
                                            if px >= rect.x
                                                && px <= rect.x + rect.width
                                                && py >= rect.y
                                                && py <= rect.y + rect.height
                                            {
                                                panel.toggle();
                                                if let Some(ref w) = self.window {
                                                    w.request_redraw();
                                                }
                                                return;
                                            }
                                        }
                                        minal_renderer::ChatHitRegion::InputArea { .. } => {
                                            // Input area click — focus is implicit.
                                        }
                                    }
                                }
                                // Click was in the panel but not on a button —
                                // consume the event so it doesn't pass through.
                                return;
                            }
                        }
                    }
                }

                self.clear_focused_ghost_text();

                let core_button = match button {
                    winit::event::MouseButton::Left => minal_core::mouse::MouseButton::Left,
                    winit::event::MouseButton::Middle => minal_core::mouse::MouseButton::Middle,
                    winit::event::MouseButton::Right => minal_core::mouse::MouseButton::Right,
                    _ => return,
                };

                if button == winit::event::MouseButton::Left {
                    self.mouse_state.left_pressed = true;

                    // Check if clicking on a divider for resize.
                    let viewport = self.content_viewport();
                    let px = self.mouse_state.pixel_pos.0 as f32;
                    let py = self.mouse_state.pixel_pos.1 as f32;
                    if let Some(ref tm) = self.tab_manager {
                        if let Some(tab) = tm.active_tab() {
                            if let Some(div) = tab.find_divider_at(viewport, px, py) {
                                self.divider_drag = Some(DividerDrag {
                                    node_path: div.node_path,
                                    direction: div.direction,
                                });
                                return;
                            }
                        }
                    }

                    // Check if clicking in a non-focused pane to switch focus.
                    let viewport = self.content_viewport();
                    if let Some(ref mut tm) = self.tab_manager {
                        if let Some(tab) = tm.active_tab_mut() {
                            if let Some(pane_id) = tab.find_pane_at(viewport, px, py) {
                                if pane_id != tab.focused_pane {
                                    tab.focused_pane = pane_id;
                                    if let Some(ref w) = self.window {
                                        w.request_redraw();
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(ref tm) = self.tab_manager {
                    if let Some(tab) = tm.active_tab() {
                        if let Some(pane) = tab.focused_pane() {
                            if let Ok(mut term) = pane.terminal.lock() {
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
                                    pane.send_io_event(IoEvent::Input(bytes));
                                } else if button == winit::event::MouseButton::Left {
                                    let click_count = self.mouse_state.register_click(col, row);
                                    use minal_core::selection::{
                                        Selection, SelectionPoint, SelectionType, word_end,
                                        word_start,
                                    };
                                    match click_count {
                                        2 => {
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
                    }
                }

                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            ElementState::Released => {
                if button == winit::event::MouseButton::Left {
                    self.mouse_state.left_pressed = false;
                    self.divider_drag = None;
                }

                let core_button = match button {
                    winit::event::MouseButton::Left => minal_core::mouse::MouseButton::Left,
                    winit::event::MouseButton::Middle => minal_core::mouse::MouseButton::Middle,
                    winit::event::MouseButton::Right => minal_core::mouse::MouseButton::Right,
                    _ => return,
                };

                let mut tracking_active = false;
                if let Some(ref tm) = self.tab_manager {
                    if let Some(tab) = tm.active_tab() {
                        if let Some(pane) = tab.focused_pane() {
                            if let Ok(term) = pane.terminal.lock() {
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
                                    pane.send_io_event(IoEvent::Input(bytes));
                                }
                            }
                        }
                    }
                }

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

        // Handle scroll in chat panel if it's visible and cursor is over it.
        if let Some(ref mut panel) = self.chat_panel {
            if panel.is_visible() && !panel.is_fully_hidden() {
                let py = self.mouse_state.pixel_pos.1 as f32;
                let (sw, sh) = self.gpu.as_ref().map_or((800, 600), |g| g.size());
                let show_tab_bar = self
                    .tab_manager
                    .as_ref()
                    .is_some_and(|tm| tm.tab_count() > 1);
                let tab_bar_h = if show_tab_bar { TAB_BAR_HEIGHT } else { 0.0 };
                #[cfg(target_os = "macos")]
                let titlebar_inset = MACOS_TITLEBAR_HEIGHT;
                #[cfg(not(target_os = "macos"))]
                let titlebar_inset: f32 = 0.0;
                let top_offset = tab_bar_h + titlebar_inset;
                let vp = panel.panel_viewport(sw as f32, sh as f32, top_offset);
                if py >= vp.y && py <= vp.y + vp.height {
                    let scroll_amount = lines.unsigned_abs() as f32 * 20.0;
                    if lines > 0 {
                        panel.scroll_up(scroll_amount);
                    } else {
                        panel.scroll_down(scroll_amount);
                    }
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }
            }
        }

        if let Some(ref tm) = self.tab_manager {
            if let Some(tab) = tm.active_tab() {
                if let Some(pane) = tab.focused_pane() {
                    if let Ok(mut term) = pane.terminal.lock() {
                        if term.mouse_tracking_active() {
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
                                pane.send_io_event(IoEvent::Input(bytes));
                            }
                        } else {
                            let count = lines.unsigned_abs() as usize;
                            if lines > 0 {
                                term.scroll_display_up(count);
                            } else {
                                term.scroll_display_down(count);
                            }
                        }
                    }
                }
            }
        }

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    /// Get current mouse modifier state from winit modifiers.
    fn current_mouse_modifiers(&self) -> minal_core::mouse::MouseModifiers {
        minal_core::mouse::MouseModifiers {
            shift: self.modifiers.shift_key(),
            alt: self.modifiers.alt_key(),
            ctrl: self.modifiers.control_key(),
        }
    }

    /// Find and handle a keybind action from a key event.
    fn find_keybind_action(&self, key_event: &winit::event::KeyEvent) -> Option<KeybindAction> {
        for binding in &self.keybind_config.bindings {
            let key_matches = match &key_event.logical_key {
                Key::Character(c) => c.as_str().eq_ignore_ascii_case(&binding.key),
                Key::Named(named) => {
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
            let mods_match = binding.modifiers.iter().all(|m| match m.as_str() {
                "Super" => self.modifiers.super_key(),
                "Ctrl" | "Control" => self.modifiers.control_key(),
                "Shift" => self.modifiers.shift_key(),
                "Alt" | "Option" => self.modifiers.alt_key(),
                _ => false,
            });
            let extra_mods = (self.modifiers.super_key() != required_super)
                || (self.modifiers.control_key() != required_ctrl)
                || (self.modifiers.shift_key() != required_shift)
                || (self.modifiers.alt_key() != required_alt);

            if mods_match && !extra_mods {
                return Some(binding.action.clone());
            }
        }
        None
    }

    /// Try to copy selected text from the focused pane to the system clipboard.
    fn clipboard_copy(&mut self) -> bool {
        let text = if let Some(ref tm) = self.tab_manager {
            if let Some(tab) = tm.active_tab() {
                if let Some(pane) = tab.focused_pane() {
                    pane.terminal.lock().ok().and_then(|t| t.selected_text())
                } else {
                    None
                }
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

    /// Paste clipboard contents into the focused pane's PTY.
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
                .tab_manager
                .as_ref()
                .and_then(|tm| tm.active_tab())
                .and_then(|tab| tab.focused_pane())
                .and_then(|pane| pane.terminal.lock().ok())
                .is_some_and(|t| t.mode(Mode::BracketedPaste));
            let mut data = Vec::new();
            if bracketed {
                data.extend_from_slice(b"\x1b[200~");
            }
            data.extend_from_slice(text.as_bytes());
            if bracketed {
                data.extend_from_slice(b"\x1b[201~");
            }
            self.send_to_focused(IoEvent::Input(data));
            self.clear_focused_ghost_text();
        }
    }

    /// Resize all panes in the active tab to match their layout viewports.
    fn resize_all_panes(&mut self) {
        let renderer = match self.renderer.as_ref() {
            Some(r) => r,
            None => return,
        };
        let (cell_width, cell_height) = renderer.cell_size();
        let padding = renderer.padding();
        let viewport = self.content_viewport();

        if let Some(ref mut tm) = self.tab_manager {
            if let Some(tab) = tm.active_tab_mut() {
                let layouts = tab.layout(viewport);
                for (pane_id, rect) in layouts {
                    let (rows, cols) = Self::compute_grid_size(
                        rect.width,
                        rect.height,
                        cell_width,
                        cell_height,
                        padding,
                    );
                    if let Some(pane) = tab.root.find_pane_mut(pane_id) {
                        if let Ok(mut term) = pane.terminal.lock() {
                            if term.rows() != rows || term.cols() != cols {
                                term.resize(rows, cols);
                                let pty_size = PtySize {
                                    rows: rows as u16,
                                    cols: cols as u16,
                                    pixel_width: rect.width as u16,
                                    pixel_height: rect.height as u16,
                                };
                                drop(term);
                                pane.send_io_event(IoEvent::Resize(pty_size));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Applies the colour palette that corresponds to `theme`.
    ///
    /// When `theme` is `None` the current window theme is used.  If
    /// `macos.follow_system_theme` is `false` this is a no-op.
    fn apply_system_theme(&mut self, theme: Option<winit::window::Theme>) {
        let follow = self
            .config
            .as_ref()
            .is_some_and(|c| c.macos.follow_system_theme);
        if !follow {
            return;
        }
        let new_theme = match theme {
            Some(winit::window::Theme::Light) => {
                self.config.as_ref().and_then(|c| c.colors_light.clone())
            }
            _ => self.config.as_ref().map(|c| c.colors.clone()),
        };
        if let Some(theme_config) = new_theme {
            if let Some(ref mut renderer) = self.renderer {
                renderer.update_theme(&theme_config);
            }
            if let Some(ref w) = self.window {
                w.request_redraw();
            }
            tracing::info!("Applied system theme: {:?}", theme);
        }
    }

    /// Apply the correct theme based on the current system appearance.
    ///
    /// Called once on launch so the renderer starts with the right palette even
    /// before a [`WindowEvent::ThemeChanged`] event arrives.
    fn apply_initial_system_theme(&mut self) {
        let system_theme = self.window.as_ref().and_then(|w| w.theme());
        self.apply_system_theme(system_theme);
    }

    /// Informs the OS of the current IME cursor position so that input method
    /// pop-up windows (candidate lists, etc.) are positioned near the cursor.
    ///
    /// This should be called after any event that moves the terminal cursor or
    /// changes the IME composition state.
    fn update_ime_cursor_area(&self) {
        let window = match self.window.as_ref() {
            Some(w) => w,
            None => return,
        };
        let renderer = match self.renderer.as_ref() {
            Some(r) => r,
            None => return,
        };

        let (cell_width, cell_height) = renderer.cell_size();
        let padding = renderer.padding();
        let viewport = self.content_viewport();

        // Obtain the focused pane's cursor position (row, col) and layout rect.
        let (cursor_row, cursor_col, pane_rect) = if let Some(ref tm) = self.tab_manager {
            if let Some(tab) = tm.active_tab() {
                let layouts = tab.layout(viewport);
                if let Some(pane) = tab.focused_pane() {
                    let (row, col) = pane
                        .terminal
                        .lock()
                        .ok()
                        .map(|t| (t.cursor().row, t.cursor().col))
                        .unwrap_or((0, 0));
                    let rect = layouts
                        .iter()
                        .find(|(pid, _)| *pid == tab.focused_pane)
                        .map(|(_, r)| *r)
                        .unwrap_or(viewport);
                    (row, col, rect)
                } else {
                    return;
                }
            } else {
                return;
            }
        } else {
            return;
        };

        let x = pane_rect.x + padding + cursor_col as f32 * cell_width;
        let y = pane_rect.y + padding + cursor_row as f32 * cell_height;
        window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(x as i32, y as i32),
            winit::dpi::PhysicalSize::new(cell_width as u32, cell_height as u32),
        );
    }

    /// Shut down all panes and clean up.
    fn shutdown(&mut self) {
        self.tab_manager = None; // Dropping TabManager drops all Tabs which drops all Panes.
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

        // Load configuration before creating the window so that macOS-specific
        // window attributes (e.g. option_as_alt) can be applied at creation time.
        let config = minal_config::Config::load().unwrap_or_else(|e| {
            tracing::warn!("Failed to load config: {e}, using defaults");
            minal_config::Config::default()
        });

        let window = match crate::window::create_window(
            event_loop,
            WINDOW_TITLE,
            DEFAULT_WIDTH,
            DEFAULT_HEIGHT,
            &config.macos,
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

        let renderer = match Renderer::new(gpu.device(), gpu.queue(), gpu.config().format, &config)
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create renderer: {e}");
                event_loop.exit();
                return;
            }
        };

        // Compute initial terminal dimensions.
        let (cell_width, cell_height) = renderer.cell_size();
        let padding = renderer.padding();
        let (rows, cols) = Self::compute_grid_size(
            phys_size.width as f32,
            phys_size.height as f32,
            cell_width,
            cell_height,
            padding,
        );
        tracing::info!("Terminal grid: {rows}x{cols} (cell: {cell_width:.1}x{cell_height:.1})");

        // Create the tab manager and spawn the first pane.
        let mut tab_manager = TabManager::new();
        let pane_id = tab_manager.next_pane_id();
        let shell = config.shell.resolve_program();

        let env_vars = shell_integration_env_vars();
        let pane = match crate::pane::Pane::spawn(
            pane_id,
            rows,
            cols,
            &shell,
            self.proxy.clone(),
            &config.ai,
            &env_vars,
        ) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to spawn initial pane: {e}");
                event_loop.exit();
                return;
            }
        };

        tab_manager.add_tab(pane);
        tracing::info!("Initial tab created");

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

        // Use default macOS keybindings merged with user config.
        let mut keybind_config = minal_config::KeybindConfig::default_macos();
        // User bindings override defaults.
        for binding in &config.keybinds.bindings {
            // Remove any default binding with the same action.
            keybind_config
                .bindings
                .retain(|b| b.action != binding.action);
            keybind_config.bindings.push(binding.clone());
        }
        self.keybind_config = keybind_config;

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

        // Initialize the chat panel if AI is enabled.
        if config.ai.enabled {
            self.chat_panel = Some(crate::chat::ChatPanelState::new(&config.ai.chat));
            tracing::info!("AI chat panel initialized");
        }

        // Initialize the error analysis panel.
        self.error_panel = Some(crate::error_panel_state::ErrorPanelState::new(0.4));

        self.config = Some(config);
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.tab_manager = Some(tab_manager);
        self.cursor_visible = true;
        self.last_blink = Instant::now();

        // Detect the initial system theme and apply the correct colour palette.
        // This mirrors the ThemeChanged handling in window_event so that the
        // renderer starts with the right colours even before any theme change event.
        self.apply_initial_system_theme();

        if let Some(ref w) = self.window {
            w.request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WakeupReason) {
        match event {
            WakeupReason::PaneUpdated(_pane_id) => {
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PaneExited(pane_id, code) => {
                tracing::info!(pane_id = pane_id.0, code, "Pane child process exited");
                if let Some(ref mut tm) = self.tab_manager {
                    tm.remove_pane(pane_id);
                    if tm.is_empty() {
                        tracing::info!("All tabs closed, exiting");
                        event_loop.exit();
                        return;
                    }
                    // Resize remaining panes.
                    self.resize_all_panes();
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PaneCompletionReady(pane_id, text) => {
                if text.is_empty() {
                    return;
                }
                tracing::debug!(pane_id = pane_id.0, completion = %text, "AI completion received");
                if let Some(ref mut tm) = self.tab_manager {
                    if let Some((_, pane)) = tm.find_pane_mut(pane_id) {
                        pane.cache_completion(&text);
                        pane.ghost_text = Some(text.clone());
                        if let Ok(mut term) = pane.terminal.lock() {
                            term.set_ghost_text(text);
                        }
                    }
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PaneCompletionFailed(pane_id) => {
                tracing::debug!(pane_id = pane_id.0, "AI completion failed");
                if let Some(ref mut tm) = self.tab_manager {
                    if let Some((_, pane)) = tm.find_pane_mut(pane_id) {
                        pane.clear_ghost_text();
                    }
                }
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
            WakeupReason::PaneClipboardSet(pane_id, text) => {
                if self.clipboard_config.osc52_write {
                    if let Some(ref mut ctx) = self.clipboard {
                        if let Err(e) = ctx.set_contents(text) {
                            tracing::warn!(
                                pane_id = pane_id.0,
                                "OSC 52: failed to set clipboard: {e}"
                            );
                        } else {
                            tracing::debug!(pane_id = pane_id.0, "OSC 52: clipboard set");
                        }
                    }
                } else {
                    tracing::debug!("OSC 52 write blocked by configuration");
                }
            }
            WakeupReason::PaneClipboardGet(pane_id) => {
                if self.clipboard_config.osc52_read {
                    if let Some(ref mut ctx) = self.clipboard {
                        match ctx.get_contents() {
                            Ok(text) => {
                                let engine = base64::engine::general_purpose::STANDARD;
                                let encoded = engine.encode(text.as_bytes());
                                let response = format!("\x1b]52;c;{encoded}\x07");
                                if let Some(ref mut tm) = self.tab_manager {
                                    if let Some((_, pane)) = tm.find_pane_mut(pane_id) {
                                        pane.send_io_event(IoEvent::Input(response.into_bytes()));
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    pane_id = pane_id.0,
                                    "OSC 52: failed to get clipboard: {e}"
                                );
                            }
                        }
                    }
                } else {
                    tracing::debug!("OSC 52 read blocked by configuration");
                }
            }
            WakeupReason::PaneChatChunk(_pane_id, text) => {
                if let Some(ref mut panel) = self.chat_panel {
                    panel.chat_engine.append_streaming_chunk(&text);
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PaneChatDone(_pane_id) => {
                if let Some(ref mut panel) = self.chat_panel {
                    let _response = panel.chat_engine.finalize_stream();
                    panel.extract_code_blocks();
                    panel.scroll_offset = 0.0;
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PaneChatError(_pane_id, error) => {
                tracing::warn!("AI chat error: {error}");
                if let Some(ref mut panel) = self.chat_panel {
                    panel.add_error_message(&error);
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PaneAnalysisReady(pane_id, analysis) => {
                tracing::info!(pane_id = pane_id.0, "Error analysis ready");
                if let Some(ref mut tm) = self.tab_manager {
                    if let Some((_tab_idx, pane)) = tm.find_pane_mut(pane_id) {
                        if let Some(ref mut analyzer) = pane.session_analyzer {
                            analyzer.update_latest_analysis(analysis);
                        }
                    }
                }
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
            }
            WakeupReason::PanePromptStarted(pane_id) => {
                tracing::debug!(pane_id = pane_id.0, "Prompt started (OSC 133;A)");
                if let Some(ref mut tm) = self.tab_manager {
                    if let Some((_tab_idx, pane)) = tm.find_pane_mut(pane_id) {
                        pane.prefetch_context();
                    }
                }
            }
            WakeupReason::AiProviderStatus(pane_id, status) => {
                tracing::info!(pane_id = pane_id.0, status = %status, "AI provider status");
                // TODO: Display in status bar when status bar UI is implemented.
            }
            WakeupReason::PaneCommandCompleted(pane_id, record) => {
                tracing::debug!(
                    pane_id = pane_id.0,
                    command = %record.command,
                    exit_code = record.exit_code,
                    "Shell command completed (OSC 133)"
                );
                if let Some(ref mut tm) = self.tab_manager {
                    if let Some((_tab_idx, pane)) = tm.find_pane_mut(pane_id) {
                        // Build a CommandRecord for both collector and analyzer.
                        let cwd = pane
                            .context_collector
                            .as_ref()
                            .and_then(|c| c.cwd().map(String::from));
                        let ai_record = minal_ai::CommandRecord {
                            command: record.command,
                            output: record.output,
                            exit_code: record.exit_code,
                            timestamp: record.timestamp,
                            cwd,
                        };

                        // Record in context collector for AI context.
                        if let Some(ref mut collector) = pane.context_collector {
                            collector.record_command(ai_record.clone());
                        }

                        // Session analysis: detect errors.
                        if let Some(ref mut analyzer) = pane.session_analyzer {
                            if let Some(detected) = analyzer.on_command_completed(&ai_record) {
                                tracing::info!(
                                    pane_id = pane_id.0,
                                    category = %detected.category,
                                    command = %detected.command,
                                    "Error detected in terminal output"
                                );

                                // Auto-request AI analysis if configured.
                                if let Some(ref cfg) = self.config {
                                    if cfg.ai.session_analysis.auto_ai_analysis {
                                        // OSC 133 does not distinguish stdout from stderr,
                                        // so we provide the combined output as stdout.
                                        let error_ctx = minal_ai::ErrorContext {
                                            command: detected.command.clone(),
                                            exit_code: detected.exit_code,
                                            stderr: String::new(),
                                            stdout: detected.output_snippet.clone(),
                                            cwd: ai_record.cwd.clone(),
                                        };
                                        pane.send_io_event(IoEvent::AiAnalyze { error: error_ctx });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            WakeupReason::MenuAction(action) => {
                tracing::debug!("Menu action received: {:?}", action);
                // Menu actions are currently informational; future work can
                // route them to the appropriate app behaviour here.
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::ModifiersChanged(mods) = &event {
            self.modifiers = mods.state();
        }

        match &event {
            WindowEvent::CloseRequested => {
                tracing::info!("Window close requested");
                self.shutdown();
                event_loop.exit();
                return;
            }

            // ── IME input ──────────────────────────────────────────────────
            WindowEvent::Ime(ime_event) => {
                match ime_event {
                    Ime::Enabled => {
                        tracing::debug!("IME enabled");
                        self.update_ime_cursor_area();
                    }
                    Ime::Disabled => {
                        tracing::debug!("IME disabled");
                        self.ime_preedit = None;
                    }
                    Ime::Preedit(text, cursor_range) => {
                        if text.is_empty() {
                            // Empty preedit signals composition cancelled.
                            self.ime_preedit = None;
                        } else {
                            self.ime_preedit = Some((text.clone(), *cursor_range));
                        }
                        self.update_ime_cursor_area();
                        if let Some(ref w) = self.window {
                            w.request_redraw();
                        }
                    }
                    Ime::Commit(text) => {
                        tracing::debug!(text = %text, "IME commit");
                        if !text.is_empty() {
                            self.send_to_focused(IoEvent::Input(text.as_bytes().to_vec()));
                            // Clear ghost text on commit, just like regular key input.
                            self.with_focused_pane(|pane| {
                                pane.clear_ghost_text();
                            });
                        }
                        self.ime_preedit = None;
                        if let Some(ref w) = self.window {
                            w.request_redraw();
                        }
                    }
                }
                return;
            }

            // ── System dark/light mode change ──────────────────────────────
            WindowEvent::ThemeChanged(theme) => {
                self.apply_system_theme(Some(*theme));
                return;
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state != ElementState::Pressed {
                    return;
                }

                // While the IME is composing (preedit is active), swallow key
                // events so they don't interfere with the composition.
                if self
                    .ime_preedit
                    .as_ref()
                    .is_some_and(|(t, _)| !t.is_empty())
                {
                    return;
                }

                // Check for keybind actions.
                if let Some(action) = self.find_keybind_action(key_event) {
                    match action {
                        KeybindAction::Copy => {
                            if self.clipboard_copy() {
                                tracing::debug!("Copied selection to clipboard");
                            }
                            return;
                        }
                        KeybindAction::Paste => {
                            self.clipboard_paste();
                            return;
                        }
                        KeybindAction::NewTab => {
                            let renderer = self.renderer.as_ref();
                            let viewport = self.content_viewport();
                            if let Some(r) = renderer {
                                let (cw, ch) = r.cell_size();
                                let padding = r.padding();
                                let (rows, cols) = Self::compute_grid_size(
                                    viewport.width,
                                    viewport.height,
                                    cw,
                                    ch,
                                    padding,
                                );
                                if let Some(pane) = self.spawn_pane(rows, cols) {
                                    if let Some(ref mut tm) = self.tab_manager {
                                        let idx = tm.add_tab(pane);
                                        tm.switch_to_tab(idx);
                                        tracing::info!("New tab created (index {idx})");
                                    }
                                }
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::CloseTab | KeybindAction::ClosePaneOrTab => {
                            if let Some(ref mut tm) = self.tab_manager {
                                if let Some(tab) = tm.active_tab_mut() {
                                    let remaining = tab.close_focused_pane();
                                    if remaining == 0 {
                                        tm.close_active_tab();
                                    }
                                }
                                if tm.is_empty() {
                                    event_loop.exit();
                                    return;
                                }
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::NextTab => {
                            if let Some(ref mut tm) = self.tab_manager {
                                tm.next_tab();
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::PrevTab => {
                            if let Some(ref mut tm) = self.tab_manager {
                                tm.prev_tab();
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::SwitchTab(n) => {
                            let idx = (n as usize).saturating_sub(1);
                            if let Some(ref mut tm) = self.tab_manager {
                                tm.switch_to_tab(idx);
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::SplitVertical => {
                            let viewport = self.content_viewport();
                            if let Some(r) = self.renderer.as_ref() {
                                let (cw, ch) = r.cell_size();
                                let padding = r.padding();
                                // After split, each side gets ~half the width.
                                let (rows, cols) = Self::compute_grid_size(
                                    viewport.width / 2.0,
                                    viewport.height,
                                    cw,
                                    ch,
                                    padding,
                                );
                                if let Some(pane) = self.spawn_pane(rows, cols) {
                                    if let Some(ref mut tm) = self.tab_manager {
                                        if let Some(tab) = tm.active_tab_mut() {
                                            let new_id = pane.id;
                                            tab.split_focused(SplitDirection::Vertical, pane);
                                            tab.focused_pane = new_id;
                                            tracing::info!("Vertical split");
                                        }
                                    }
                                }
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::SplitHorizontal => {
                            let viewport = self.content_viewport();
                            if let Some(r) = self.renderer.as_ref() {
                                let (cw, ch) = r.cell_size();
                                let padding = r.padding();
                                let (rows, cols) = Self::compute_grid_size(
                                    viewport.width,
                                    viewport.height / 2.0,
                                    cw,
                                    ch,
                                    padding,
                                );
                                if let Some(pane) = self.spawn_pane(rows, cols) {
                                    if let Some(ref mut tm) = self.tab_manager {
                                        if let Some(tab) = tm.active_tab_mut() {
                                            let new_id = pane.id;
                                            tab.split_focused(SplitDirection::Horizontal, pane);
                                            tab.focused_pane = new_id;
                                            tracing::info!("Horizontal split");
                                        }
                                    }
                                }
                            }
                            self.resize_all_panes();
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::FocusNextPane => {
                            if let Some(ref mut tm) = self.tab_manager {
                                if let Some(tab) = tm.active_tab_mut() {
                                    tab.focus_next_pane();
                                }
                            }
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::FocusPrevPane => {
                            if let Some(ref mut tm) = self.tab_manager {
                                if let Some(tab) = tm.active_tab_mut() {
                                    tab.focus_prev_pane();
                                }
                            }
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::AiToggleErrorPanel => {
                            // Close chat panel if open (mutual exclusion).
                            if let Some(ref mut chat) = self.chat_panel {
                                if chat.is_visible() {
                                    chat.toggle();
                                }
                            }
                            if let Some(ref mut panel) = self.error_panel {
                                panel.toggle();
                                tracing::info!(
                                    "Error panel toggled: {}",
                                    if panel.is_visible() { "open" } else { "closed" }
                                );
                            }
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::AiToggleChat => {
                            // Close error panel if open (mutual exclusion).
                            if let Some(ref mut ep) = self.error_panel {
                                if ep.is_visible() {
                                    ep.close();
                                }
                            }
                            if let Some(ref mut panel) = self.chat_panel {
                                panel.toggle();
                                tracing::info!(
                                    "Chat panel toggled: {}",
                                    if panel.is_visible() { "open" } else { "closed" }
                                );
                            }
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        KeybindAction::AiToggle => {
                            self.with_focused_pane(|pane| {
                                if let Some(ref mut engine) = pane.completion_engine {
                                    engine.toggle();
                                    let enabled = engine.is_enabled();
                                    tracing::info!(
                                        "AI completion toggled: {}",
                                        if enabled { "on" } else { "off" }
                                    );
                                    if !enabled {
                                        pane.clear_ghost_text();
                                    }
                                }
                            });
                            if let Some(ref w) = self.window {
                                w.request_redraw();
                            }
                            return;
                        }
                        _ => {} // Other keybind actions fall through to normal handling.
                    }
                }

                // ── Error panel input handling ─────────────────────────
                if self.error_panel.as_ref().is_some_and(|p| p.is_visible()) {
                    match key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            if let Some(ref mut panel) = self.error_panel {
                                panel.close();
                            }
                        }
                        Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::PageUp) => {
                            if let Some(ref mut panel) = self.error_panel {
                                panel.scroll_up(40.0);
                            }
                        }
                        Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::PageDown) => {
                            if let Some(ref mut panel) = self.error_panel {
                                panel.scroll_down(40.0);
                            }
                        }
                        _ => {}
                    }
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }

                // ── Chat panel input handling ─────────────────────────
                if self.chat_panel.as_ref().is_some_and(|p| p.is_visible()) {
                    self.handle_chat_key_input(key_event);
                    return;
                }

                // Tab when ghost text is active: accept completion.
                if self.focused_has_ghost_text()
                    && matches!(key_event.logical_key, Key::Named(NamedKey::Tab))
                {
                    self.with_focused_pane(|pane| {
                        if let Some(text) = pane.ghost_text.take() {
                            tracing::debug!(accepted = %text, "AI completion accepted");
                            pane.send_io_event(IoEvent::Input(text.into_bytes()));
                            pane.clear_ghost_text();
                            if let Some(ref mut engine) = pane.completion_engine {
                                engine.clear();
                            }
                        }
                    });
                    self.cursor_visible = true;
                    self.last_blink = Instant::now();
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }

                // Escape when ghost text is active: dismiss completion.
                if self.focused_has_ghost_text()
                    && matches!(key_event.logical_key, Key::Named(NamedKey::Escape))
                {
                    self.with_focused_pane(|pane| {
                        pane.clear_ghost_text();
                        if let Some(ref mut engine) = pane.completion_engine {
                            engine.clear();
                        }
                    });
                    if let Some(ref w) = self.window {
                        w.request_redraw();
                    }
                    return;
                }

                // Normal key input -> send to focused pane.
                if let Some(bytes) = self.translate_key_input(key_event) {
                    self.send_to_focused(IoEvent::Input(bytes));

                    // Clear selection on keyboard input.
                    self.with_focused_pane(|pane| {
                        if let Ok(mut term) = pane.terminal.lock() {
                            term.set_selection(None);
                        }
                        pane.clear_ghost_text();
                        pane.notify_completion_engine();
                    });

                    self.cursor_visible = true;
                    self.last_blink = Instant::now();
                    // Update the IME cursor area so that candidate pop-ups appear
                    // near the new cursor position after movement.
                    self.update_ime_cursor_area();
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
                self.check_focused_debounce();
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
                gpu.resize(physical_size.width, physical_size.height);
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
                self.resize_all_panes();
            }

            WindowEvent::ScaleFactorChanged { .. } => {
                let new_size = window.inner_size();
                gpu.resize(new_size.width, new_size.height);
                if let Some(ref w) = self.window {
                    w.request_redraw();
                }
                self.resize_all_panes();
            }

            WindowEvent::RedrawRequested => {
                // Update chat panel animation.
                let now = Instant::now();
                let dt = now.duration_since(self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;
                let chat_animating = self
                    .chat_panel
                    .as_mut()
                    .is_some_and(|p| p.update_animation(dt));
                let error_panel_animating = self
                    .error_panel
                    .as_mut()
                    .is_some_and(|p| p.update_animation(dt));

                // Update cursor blink state.
                let blink_interval = Duration::from_millis(CURSOR_BLINK_MS);
                if now.duration_since(self.last_blink) >= blink_interval {
                    self.cursor_visible = !self.cursor_visible;
                    self.last_blink = now;
                }

                let Some(ref tab_manager) = self.tab_manager else {
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
                let show_tab_bar = tab_manager.tab_count() > 1;
                let tab_bar_h = if show_tab_bar { TAB_BAR_HEIGHT } else { 0.0 };

                let content_viewport = Rect {
                    x: 0.0,
                    y: tab_bar_h,
                    width: w as f32,
                    height: h as f32 - tab_bar_h,
                };

                let tab_infos: Vec<TabBarInfo> = tab_manager
                    .tab_render_info()
                    .into_iter()
                    .map(|t| TabBarInfo {
                        title: t.title,
                        is_active: t.is_active,
                    })
                    .collect();

                // Compute divider rects.
                let divider_rects: Vec<(f32, f32, f32, f32)> =
                    if let Some(tab) = tab_manager.active_tab() {
                        tab.dividers(content_viewport)
                            .iter()
                            .map(|d| (d.rect.x, d.rect.y, d.rect.width, d.rect.height))
                            .collect()
                    } else {
                        Vec::new()
                    };

                // Find the focused pane's viewport for the focus indicator.
                let focused_viewport = if let Some(tab) = tab_manager.active_tab() {
                    let layouts = tab.layout(content_viewport);
                    let pane_count = layouts.len();
                    layouts
                        .into_iter()
                        .find(|(pid, _)| *pid == tab.focused_pane)
                        .and_then(|(_, rect)| {
                            if pane_count > 1 {
                                Some(Viewport {
                                    x: rect.x,
                                    y: rect.y,
                                    width: rect.width,
                                    height: rect.height,
                                })
                            } else {
                                None
                            }
                        })
                } else {
                    None
                };

                let cursor_visible = self.cursor_visible;

                // Collect pane render data (we need to lock terminals).
                struct PaneRenderData {
                    viewport: Viewport,
                    is_focused: bool,
                }

                let pane_data: Vec<(PaneId, PaneRenderData)> =
                    if let Some(tab) = tab_manager.active_tab() {
                        tab.layout(content_viewport)
                            .into_iter()
                            .map(|(pid, rect)| {
                                (
                                    pid,
                                    PaneRenderData {
                                        viewport: Viewport {
                                            x: rect.x,
                                            y: rect.y,
                                            width: rect.width,
                                            height: rect.height,
                                        },
                                        is_focused: pid == tab.focused_pane,
                                    },
                                )
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };

                // Extract chat panel render data before entering the closure.
                let chat_render_data = self.chat_panel.as_ref().and_then(|panel| {
                    if panel.is_fully_hidden() {
                        return None;
                    }
                    let tab_bar_h = if show_tab_bar { TAB_BAR_HEIGHT } else { 0.0 };
                    #[cfg(target_os = "macos")]
                    let titlebar_inset = MACOS_TITLEBAR_HEIGHT;
                    #[cfg(not(target_os = "macos"))]
                    let titlebar_inset: f32 = 0.0;
                    let top_offset = tab_bar_h + titlebar_inset;

                    let panel_vp = panel.panel_viewport(w as f32, h as f32, top_offset);
                    let messages = panel.render_messages();
                    let streaming_text = panel.chat_engine.streaming_buffer().to_string();
                    let input_text = panel.input_buffer.clone();
                    let input_cursor = panel.input_cursor;
                    let scroll_offset = panel.scroll_offset;
                    let is_streaming = panel.chat_engine.is_streaming();

                    Some((
                        panel_vp,
                        messages,
                        streaming_text,
                        input_text,
                        input_cursor,
                        scroll_offset,
                        is_streaming,
                    ))
                });

                // Extract error panel render data before entering the closure.
                let error_panel_data = self.error_panel.as_ref().and_then(|panel| {
                    if panel.is_fully_hidden() {
                        return None;
                    }
                    let tab_bar_h = if show_tab_bar { TAB_BAR_HEIGHT } else { 0.0 };
                    #[cfg(target_os = "macos")]
                    let titlebar_inset = MACOS_TITLEBAR_HEIGHT;
                    #[cfg(not(target_os = "macos"))]
                    let titlebar_inset: f32 = 0.0;
                    let top_offset = tab_bar_h + titlebar_inset;
                    let panel_vp = panel.panel_viewport(w as f32, h as f32, top_offset);
                    let scroll_offset = panel.scroll_offset;
                    Some((panel_vp, scroll_offset))
                });

                // Collect error entries from the focused pane for rendering.
                let error_entries: Vec<minal_renderer::ErrorPanelEntry> =
                    if error_panel_data.is_some() {
                        if let Some(tab) = tab_manager.active_tab() {
                            if let Some(pane) = tab.focused_pane() {
                                if let Some(ref analyzer) = pane.session_analyzer {
                                    analyzer
                                        .errors()
                                        .map(|e| minal_renderer::ErrorPanelEntry {
                                            category: e.category.to_string(),
                                            command: e.command.clone(),
                                            summary: e.summary.clone(),
                                            explanation: e
                                                .ai_analysis
                                                .as_ref()
                                                .map(|a| a.explanation.clone()),
                                            suggestions: e
                                                .ai_analysis
                                                .as_ref()
                                                .map(|a| a.suggestions.clone())
                                                .unwrap_or_default(),
                                        })
                                        .collect()
                                } else {
                                    Vec::new()
                                }
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };

                // Count total errors for badge across all panes in active tab.
                let total_error_count: usize = if let Some(tab) = tab_manager.active_tab() {
                    tab.pane_ids()
                        .iter()
                        .filter_map(|pid| {
                            tab.root
                                .find_pane(*pid)
                                .and_then(|p| p.session_analyzer.as_ref().map(|a| a.error_count()))
                        })
                        .sum()
                } else {
                    0
                };

                renderer.render_multi_pane(
                    gpu.device(),
                    gpu.queue(),
                    &frame.view,
                    w,
                    h,
                    &tab_infos,
                    show_tab_bar,
                    &divider_rects,
                    focused_viewport,
                    |renderer, rect_instances, text_instances| {
                        if let Some(tab) = tab_manager.active_tab() {
                            for (pid, data) in &pane_data {
                                if let Some(pane) = tab.root.find_pane(*pid) {
                                    if let Ok(mut term) = pane.terminal.lock() {
                                        let mut cursor = term.cursor().clone();
                                        if !data.is_focused || !cursor_visible {
                                            cursor.visible = data.is_focused && cursor_visible;
                                        }

                                        let ghost = term.ghost_text();
                                        let selection = term.selection();
                                        renderer.build_pane_instances(
                                            data.viewport,
                                            term.grid(),
                                            &cursor,
                                            ghost,
                                            selection,
                                            rect_instances,
                                            text_instances,
                                        );

                                        term.clear_dirty();
                                    }
                                }
                            }
                        }

                        // Render chat panel overlay on top of terminal content.
                        if let Some((
                            panel_vp,
                            ref messages,
                            ref streaming_text,
                            ref input_text,
                            input_cursor,
                            scroll_offset,
                            is_streaming,
                        )) = chat_render_data
                        {
                            renderer.build_chat_panel_instances(
                                panel_vp,
                                messages,
                                streaming_text,
                                input_text,
                                input_cursor,
                                scroll_offset,
                                is_streaming,
                                rect_instances,
                                text_instances,
                            );
                        }

                        // Render error panel overlay.
                        if let Some((panel_vp, scroll_offset)) = error_panel_data {
                            renderer.build_error_panel_instances(
                                panel_vp,
                                &error_entries,
                                scroll_offset,
                                rect_instances,
                                text_instances,
                            );
                        }

                        // Render error badge.
                        if total_error_count > 0 {
                            renderer.build_error_badge_instances(
                                w as f32,
                                h as f32,
                                total_error_count,
                                rect_instances,
                                text_instances,
                            );
                        }
                    },
                );

                // Store hit regions from the render pass.
                // The renderer returns them, but since we're inside a closure
                // we need to handle this differently. For now, we skip storing
                // hit regions from the closure and rely on viewport-based hit testing.

                frame.present();

                // Schedule the next wakeup.
                let next_blink = self.last_blink + blink_interval;
                let mut next_wakeup = next_blink;

                // If the chat or error panel is animating, request continuous redraws.
                if chat_animating || error_panel_animating {
                    next_wakeup = now + Duration::from_millis(16);
                }

                // Check all focused pane's debounce deadline.
                if let Some(ref tm) = self.tab_manager {
                    if let Some(tab) = tm.active_tab() {
                        if let Some(pane) = tab.focused_pane() {
                            if let Some(ref engine) = pane.completion_engine {
                                if let Some(debounce_deadline) = engine.debounce_deadline() {
                                    if debounce_deadline < next_wakeup {
                                        next_wakeup = debounce_deadline;
                                    }
                                }
                            }
                        }
                    }
                }

                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_wakeup));
            }

            _ => {}
        }
    }
}
