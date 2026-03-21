//! Per-pane state: terminal, PTY, I/O thread, and AI completion.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::Sender;
use winit::event_loop::EventLoopProxy;

use minal_ai::{CompletionEngine, ContextCollector};
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
    /// Context collector for AI requests.
    pub context_collector: Option<ContextCollector>,
    /// Current ghost text suggestion from AI.
    pub ghost_text: Option<String>,
    /// Pending AI context for caching the response when it arrives.
    pub(crate) pending_context: Option<minal_ai::AiContext>,
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

        let (completion_engine, context_collector) = if ai_config.enabled {
            let mut collector = ContextCollector::new(ai_config.privacy.clone());
            // Set initial CWD from the current process (child inherits it).
            if let Ok(cwd) = std::env::current_dir() {
                collector.set_cwd(cwd.to_string_lossy().to_string());
            }
            (
                Some(CompletionEngine::new(
                    ai_config.debounce_ms,
                    ai_config.completion_cache_size,
                )),
                Some(collector),
            )
        } else {
            (None, None)
        };

        Ok(Self {
            id,
            terminal,
            io_tx,
            io_thread: Some(io_thread),
            completion_engine,
            context_collector,
            ghost_text: None,
            pending_context: None,
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

    /// Cache a received completion for future reuse.
    pub fn cache_completion(&mut self, completion: &str) {
        if let Some(ctx) = self.pending_context.take() {
            if let Some(ref mut engine) = self.completion_engine {
                engine.cache_completion(&ctx, completion.to_string());
            }
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

    /// Pre-gather context when a new prompt is detected (OSC 133;A).
    ///
    /// Eagerly collects CWD, git info, and project type so the first
    /// completion request after the prompt appears is faster.
    pub fn prefetch_context(&mut self) {
        if let Some(ref mut engine) = self.completion_engine {
            engine.on_prompt_detected();
            if let Ok(term) = self.terminal.lock() {
                if let Some(ref collector) = self.context_collector {
                    let ctx = collector.gather(&term);
                    engine.set_prefetched_context(ctx);
                }
            }
        }
    }

    /// Check debounce and possibly trigger an AI completion request.
    ///
    /// First checks the LRU cache; on a hit the ghost text is set directly
    /// without a network round-trip. On a miss, sends `IoEvent::AiComplete`
    /// to the I/O thread. Uses prefetched context when available.
    pub fn check_debounce_and_request(&mut self) {
        let prefix = if let Some(ref mut engine) = self.completion_engine {
            engine.tick()
        } else {
            None
        };

        if let Some(prefix) = prefix {
            // Build context, preferring prefetched context.
            let context = if let Some(ref mut engine) = self.completion_engine {
                if let Some(mut ctx) = engine.take_prefetched_context() {
                    ctx.input_prefix = prefix.clone();
                    ctx
                } else if let Ok(term) = self.terminal.lock() {
                    if let Some(ref collector) = self.context_collector {
                        let mut ctx = collector.gather(&term);
                        ctx.input_prefix = prefix.clone();
                        ctx
                    } else {
                        minal_ai::AiContext {
                            input_prefix: prefix.clone(),
                            ..Default::default()
                        }
                    }
                } else {
                    minal_ai::AiContext {
                        input_prefix: prefix.clone(),
                        ..Default::default()
                    }
                }
            } else {
                minal_ai::AiContext {
                    input_prefix: prefix.clone(),
                    ..Default::default()
                }
            };

            // Check cache before making a network request.
            if let Some(ref mut engine) = self.completion_engine {
                if let Some(cached) = engine.check_cache(&context) {
                    tracing::debug!(
                        pane_id = self.id.0,
                        prefix = %prefix,
                        "AI completion cache hit"
                    );
                    self.ghost_text = Some(cached.clone());
                    if let Ok(mut term) = self.terminal.lock() {
                        term.set_ghost_text(cached);
                    }
                    return;
                }
            }

            // Store context for caching the response later.
            self.pending_context = Some(context.clone());

            tracing::debug!(pane_id = self.id.0, prefix = %prefix, "Requesting AI completion");
            self.send_io_event(IoEvent::AiComplete { context });
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
