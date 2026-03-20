//! Per-pane state: terminal, PTY, I/O thread, and AI completion.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::Sender;
use winit::event_loop::EventLoopProxy;

use minal_ai::CompletionEngine;
use minal_core::pty::{Pty, PtySize};
use minal_core::term::Terminal;

use crate::event::{IoEvent, WakeupReason};
use crate::io::pane_io_loop;

/// Unique identifier for a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u64);

/// A single terminal pane with its own PTY and I/O thread.
pub struct Pane {
    /// Unique identifier.
    pub id: PaneId,
    /// Terminal state (shared with the pane's I/O thread).
    pub terminal: Arc<Mutex<Terminal>>,
    /// Channel to send events to this pane's I/O thread.
    pub io_tx: Sender<IoEvent>,
    /// Handle to the I/O thread (taken on shutdown).
    pub(crate) io_thread: Option<JoinHandle<()>>,
    /// AI completion engine for this pane.
    pub completion_engine: Option<CompletionEngine>,
    /// Current ghost text suggestion from AI.
    pub ghost_text: Option<String>,
    /// Tab title derived from this pane.
    pub title: String,
}

impl Pane {
    /// Spawns a new pane with its own terminal, PTY, and I/O thread.
    ///
    /// # Errors
    /// Returns an error if PTY or I/O thread creation fails.
    pub fn spawn(
        id: PaneId,
        rows: usize,
        cols: usize,
        shell: &str,
        proxy: EventLoopProxy<WakeupReason>,
        ai_config: &minal_config::AiConfig,
        env_vars: &[(String, String)],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let terminal = Arc::new(Mutex::new(Terminal::new(rows, cols)));
        let pty_size = PtySize::new(rows as u16, cols as u16);
        let pty = Pty::open(shell, pty_size, env_vars)?;
        tracing::info!(
            pane_id = id.0,
            child_pid = pty.child_pid(),
            "Pane PTY opened"
        );

        let (io_tx, io_rx) = crossbeam_channel::unbounded::<IoEvent>();

        let terminal_clone = Arc::clone(&terminal);
        let ai_config_clone = ai_config.clone();
        let pane_id = id;
        let io_thread = std::thread::Builder::new()
            .name(format!("minal-io-{}", id.0))
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!(pane_id = pane_id.0, "Failed to create tokio runtime: {e}");
                        return;
                    }
                };
                rt.block_on(pane_io_loop(
                    pane_id,
                    pty,
                    io_rx,
                    terminal_clone,
                    proxy,
                    ai_config_clone,
                ));
            })?;

        let completion_engine = if ai_config.enabled {
            Some(CompletionEngine::new(ai_config.debounce_ms))
        } else {
            None
        };

        Ok(Self {
            id,
            terminal,
            io_tx,
            io_thread: Some(io_thread),
            completion_engine,
            ghost_text: None,
            title: std::path::Path::new(shell)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(shell)
                .to_string(),
        })
    }

    /// Send an I/O event to this pane's I/O thread, logging on failure.
    pub fn send_io_event(&self, event: IoEvent) {
        if let Err(e) = self.io_tx.send(event) {
            tracing::warn!(pane_id = self.id.0, "Failed to send I/O event: {e}");
        }
    }

    /// Clear ghost text state and remove it from the terminal.
    pub fn clear_ghost_text(&mut self) {
        self.ghost_text = None;
        if let Ok(mut term) = self.terminal.lock() {
            term.clear_ghost_text();
        }
    }

    /// Notify the completion engine of the current input line.
    pub fn notify_completion_engine(&mut self) {
        if let Some(ref mut engine) = self.completion_engine {
            if let Ok(term) = self.terminal.lock() {
                let prefix = term.cursor_line_prefix();
                engine.on_input_changed(&prefix);
            }
        }
    }

    /// Check debounce and possibly trigger an AI completion request.
    pub fn check_debounce_and_request(&mut self) {
        let prefix = if let Some(ref mut engine) = self.completion_engine {
            engine.tick()
        } else {
            None
        };

        if let Some(prefix) = prefix {
            let recent_output = if let Ok(term) = self.terminal.lock() {
                let gatherer = minal_ai::ContextGatherer::default();
                let ctx = gatherer.gather(&term);
                ctx.recent_output
            } else {
                Vec::new()
            };

            tracing::debug!(pane_id = self.id.0, prefix = %prefix, "Requesting AI completion");
            self.send_io_event(IoEvent::AiComplete {
                prefix,
                recent_output,
            });
        }
    }

    /// Shut down this pane's I/O thread gracefully.
    pub fn shutdown(&mut self) {
        self.send_io_event(IoEvent::Shutdown);
        if let Some(handle) = self.io_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.shutdown();
    }
}
