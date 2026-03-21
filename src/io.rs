//! Per-pane I/O loop running on a dedicated thread.
//!
//! Each pane spawns its own I/O thread with a tokio runtime that handles:
//! - PTY read/write
//! - VT parsing
//! - AI completion requests

use std::sync::{Arc, Mutex};

use tokio_stream::StreamExt as _;
use winit::event_loop::EventLoopProxy;

use minal_ai::provider::AiProvider;
use minal_core::handler::Handler;
use minal_core::pty::{AsyncPty, Pty};
use minal_core::term::Terminal;

use crate::event::{IoEvent, WakeupReason};
use crate::pane::PaneId;

/// The async I/O loop for a single pane.
///
/// Reads PTY output, feeds it through the VT parser to update terminal state,
/// and listens for commands from the main thread. Events sent back to the main
/// thread carry the `pane_id` so the main thread knows which pane triggered them.
pub async fn pane_io_loop(
    pane_id: PaneId,
    pty: Pty,
    io_rx: crossbeam_channel::Receiver<IoEvent>,
    terminal: Arc<Mutex<Terminal>>,
    proxy: EventLoopProxy<WakeupReason>,
    ai_config: minal_config::AiConfig,
) {
    let async_pty = match AsyncPty::from_pty(pty) {
        Ok(ap) => ap,
        Err(e) => {
            tracing::error!(pane_id = pane_id.0, "Failed to create AsyncPty: {e}");
            let _ = proxy.send_event(WakeupReason::PaneExited(pane_id, 1));
            return;
        }
    };

    // Create AI provider if enabled.
    let ai_provider: Option<Arc<dyn AiProvider>> = if ai_config.enabled {
        let keystore = minal_ai::default_keystore(&ai_config);
        match minal_ai::create_provider(&ai_config, &*keystore) {
            Ok(provider) => {
                tracing::info!(
                    pane_id = pane_id.0,
                    provider = provider.name(),
                    "AI provider created"
                );
                Some(provider)
            }
            Err(e) => {
                tracing::warn!(pane_id = pane_id.0, "Failed to create AI provider: {e}");
                None
            }
        }
    } else {
        None
    };
    // Warm up Ollama if configured and using Ollama provider.
    if let Some(ref provider) = ai_provider {
        if ai_config.ollama_warmup
            && matches!(ai_config.provider, minal_config::AiProviderKind::Ollama)
        {
            let provider = Arc::clone(provider);
            let proxy_clone = proxy.clone();
            let pid = pane_id;
            tokio::spawn(async move {
                match provider.warmup().await {
                    Ok(()) => {
                        tracing::info!(pane_id = pid.0, "AI provider warmup succeeded");
                    }
                    Err(e) => {
                        tracing::warn!(pane_id = pid.0, "AI provider warmup failed: {e}");
                        let _ = proxy_clone.send_event(WakeupReason::AiProviderStatus(
                            pid,
                            format!("Warmup failed: {e}"),
                        ));
                    }
                }
            });
        }
    }

    // Start Ollama memory monitoring if configured.
    if ai_config.enabled && matches!(ai_config.provider, minal_config::AiProviderKind::Ollama) {
        if let Some(limit_mb) = ai_config.ollama_memory_limit_mb {
            match minal_ai::ollama_health::OllamaHealthChecker::new(
                ai_config.base_url.clone(),
                limit_mb,
            ) {
                Ok(checker) => {
                    let proxy_clone = proxy.clone();
                    let pid = pane_id;
                    tokio::spawn(async move {
                        let mut interval =
                            tokio::time::interval(std::time::Duration::from_secs(60));
                        loop {
                            interval.tick().await;
                            match checker.check_memory_usage_mb().await {
                                Ok(usage_mb) if usage_mb > checker.memory_limit_mb() => {
                                    tracing::warn!(
                                        usage_mb,
                                        limit_mb = checker.memory_limit_mb(),
                                        "Ollama memory exceeds limit"
                                    );
                                    let _ = proxy_clone.send_event(WakeupReason::AiProviderStatus(
                                        pid,
                                        format!(
                                            "Ollama memory usage: {}MB (limit: {}MB)",
                                            usage_mb,
                                            checker.memory_limit_mb()
                                        ),
                                    ));
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    tracing::debug!("Ollama health check failed: {e}");
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to create Ollama health checker: {e}");
                }
            }
        }
    }

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
                        tracing::info!(pane_id = pane_id.0, "PTY EOF, child process ended");
                        let code = async_pty.try_wait()
                            .ok()
                            .flatten()
                            .unwrap_or(0);
                        let _ = proxy.send_event(WakeupReason::PaneExited(pane_id, code));
                        return;
                    }
                    Ok(n) => {
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
                                            WakeupReason::PaneClipboardSet(pane_id, text),
                                        );
                                    }
                                    minal_core::term::ClipboardAction::RequestContents => {
                                        let _ = proxy.send_event(
                                            WakeupReason::PaneClipboardGet(pane_id),
                                        );
                                    }
                                }
                            }
                            // Check for pending shell integration events from OSC 133.
                            for shell_event in term.take_pending_shell_events() {
                                match shell_event {
                                    minal_core::shell_integration::ShellEvent::CommandCompleted(record) => {
                                        let _ = proxy.send_event(
                                            WakeupReason::PaneCommandCompleted(pane_id, record),
                                        );
                                    }
                                    minal_core::shell_integration::ShellEvent::PromptStarted => {
                                        let _ = proxy.send_event(
                                            WakeupReason::PanePromptStarted(pane_id),
                                        );
                                    }
                                }
                            }
                            drop(term);
                            let _ = proxy.send_event(WakeupReason::PaneUpdated(pane_id));
                        }
                    }
                    Err(e) => {
                        tracing::error!(pane_id = pane_id.0, "PTY read error: {e}");
                        let code = async_pty.try_wait()
                            .ok()
                            .flatten()
                            .unwrap_or(1);
                        let _ = proxy.send_event(WakeupReason::PaneExited(pane_id, code));
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
                                    tracing::error!(pane_id = pane_id.0, "PTY write error: {e}");
                                    break;
                                }
                            }
                        }
                    }
                    Some(IoEvent::Resize(size)) => {
                        if let Err(e) = async_pty.resize(size) {
                            tracing::warn!(pane_id = pane_id.0, "PTY resize failed: {e}");
                        }
                    }
                    Some(IoEvent::AiComplete { context }) => {
                        if let Some(ref provider) = ai_provider {
                            if let Some(task) = ai_task.take() {
                                task.abort();
                            }
                            let provider = Arc::clone(provider);
                            let proxy_clone = proxy.clone();
                            let pid = pane_id;
                            ai_task = Some(tokio::spawn(async move {
                                match provider.complete(&context).await {
                                    Ok(completion) if !completion.is_empty() => {
                                        let _ = proxy_clone.send_event(
                                            WakeupReason::PaneCompletionReady(pid, completion),
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(e) => {
                                        tracing::debug!(pane_id = pid.0, "AI completion error: {e}");
                                        let _ = proxy_clone.send_event(
                                            WakeupReason::PaneCompletionFailed(pid),
                                        );
                                    }
                                }
                            }));
                        }
                    }
                    Some(IoEvent::AiChat { messages, context }) => {
                        if let Some(ref provider) = ai_provider {
                            if let Some(task) = ai_task.take() {
                                task.abort();
                            }
                            let provider = Arc::clone(provider);
                            let proxy_clone = proxy.clone();
                            let pid = pane_id;
                            ai_task = Some(tokio::spawn(async move {
                                match provider.chat_stream(&messages, &context).await {
                                    Ok(mut stream) => {
                                        while let Some(chunk) = stream.next().await {
                                            match chunk {
                                                Ok(text) => {
                                                    let _ = proxy_clone.send_event(
                                                        WakeupReason::PaneChatChunk(pid, text),
                                                    );
                                                }
                                                Err(e) => {
                                                    let _ = proxy_clone.send_event(
                                                        WakeupReason::PaneChatError(
                                                            pid,
                                                            e.to_string(),
                                                        ),
                                                    );
                                                    return;
                                                }
                                            }
                                        }
                                        let _ = proxy_clone
                                            .send_event(WakeupReason::PaneChatDone(pid));
                                    }
                                    Err(e) => {
                                        let _ = proxy_clone.send_event(
                                            WakeupReason::PaneChatError(pid, e.to_string()),
                                        );
                                    }
                                }
                            }));
                        }
                    }
                    Some(IoEvent::AiAnalyze { error }) => {
                        if let Some(ref provider) = ai_provider {
                            let provider = Arc::clone(provider);
                            let proxy_clone = proxy.clone();
                            let pid = pane_id;
                            tokio::spawn(async move {
                                match provider.analyze_error(&error).await {
                                    Ok(analysis) => {
                                        let _ = proxy_clone.send_event(
                                            WakeupReason::PaneAnalysisReady(pid, analysis),
                                        );
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            pane_id = pid.0,
                                            "AI analysis error: {e}"
                                        );
                                    }
                                }
                            });
                        }
                    }
                    Some(IoEvent::Shutdown) | None => {
                        tracing::info!(pane_id = pane_id.0, "I/O thread shutting down");
                        return;
                    }
                }
            }
        }
    }
}
