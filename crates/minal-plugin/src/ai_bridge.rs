//! Bridge from WASM plugins to the [`AiProvider`] trait.
//!
//! A [`WasmAiProvider`] communicates with a plugin instance running on a
//! dedicated thread via a request/response channel.  This avoids the need
//! for `unsafe impl Send` on the wasmtime `Store`.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::oneshot;

use minal_ai::AiError;
use minal_ai::provider::AiProvider;
use minal_ai::types::{AiContext, ErrorAnalysis, ErrorContext, Message};

use crate::error::PluginError;
use crate::runtime::PluginInstance;

/// Request sent from the provider to the plugin worker thread.
enum AiRequest {
    Complete {
        context_json: String,
        reply: oneshot::Sender<Result<String, PluginError>>,
    },
    AnalyzeError {
        error_json: String,
        reply: oneshot::Sender<Result<Option<String>, PluginError>>,
    },
    IsAvailable {
        reply: oneshot::Sender<bool>,
    },
}

/// An AI provider backed by a WASM plugin.
///
/// The plugin instance lives on a dedicated background thread and is
/// accessed via a channel.  This avoids any `unsafe` thread-safety hacks.
pub struct WasmAiProvider {
    /// Display name for this provider.
    name: String,
    /// Channel to send requests to the worker thread.
    sender: Arc<std::sync::mpsc::Sender<AiRequest>>,
}

impl WasmAiProvider {
    /// Create a new WASM-backed AI provider.
    ///
    /// Spawns a background thread that owns the `PluginInstance` and
    /// processes AI requests.
    pub fn new(name: String, mut instance: PluginInstance) -> Result<Self, PluginError> {
        let (tx, rx) = std::sync::mpsc::channel::<AiRequest>();

        std::thread::Builder::new()
            .name(format!("minal-plugin-ai-{name}"))
            .spawn(move || {
                while let Ok(req) = rx.recv() {
                    match req {
                        AiRequest::Complete {
                            context_json,
                            reply,
                        } => {
                            let result = instance.call_ai_complete(&context_json);
                            let mapped = result.and_then(|opt| {
                                opt.ok_or_else(|| {
                                    PluginError::Call(
                                        "plugin did not return a completion".to_string(),
                                    )
                                })
                            });
                            let _ = reply.send(mapped);
                        }
                        AiRequest::AnalyzeError { error_json, reply } => {
                            let result = instance.call_ai_analyze_error(&error_json);
                            let _ = reply.send(result);
                        }
                        AiRequest::IsAvailable { reply } => {
                            let available = instance.has_export("minal_ai_complete");
                            let _ = reply.send(available);
                        }
                    }
                }
                tracing::debug!("plugin AI worker thread exiting");
            })
            .map_err(PluginError::ThreadSpawn)?;

        Ok(Self {
            name,
            sender: Arc::new(tx),
        })
    }
}

impl std::fmt::Debug for WasmAiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmAiProvider")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl AiProvider for WasmAiProvider {
    async fn complete(&self, context: &AiContext) -> Result<String, AiError> {
        let context_json =
            serde_json::to_string(context).map_err(|e| AiError::Provider(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.sender
            .send(AiRequest::Complete {
                context_json,
                reply: tx,
            })
            .map_err(|_| AiError::Provider("plugin worker thread is gone".to_string()))?;

        rx.await
            .map_err(|_| AiError::Provider("plugin worker did not reply".to_string()))?
            .map_err(|e| AiError::Provider(format!("plugin error: {e}")))
    }

    async fn chat_stream(
        &self,
        _messages: &[Message],
        _context: &AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError> {
        Err(AiError::Provider(
            "WASM plugin AI providers do not support streaming chat".to_string(),
        ))
    }

    async fn analyze_error(&self, error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
        let error_json =
            serde_json::to_string(error).map_err(|e| AiError::Provider(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.sender
            .send(AiRequest::AnalyzeError {
                error_json,
                reply: tx,
            })
            .map_err(|_| AiError::Provider("plugin worker thread is gone".to_string()))?;

        let result = rx
            .await
            .map_err(|_| AiError::Provider("plugin worker did not reply".to_string()))?
            .map_err(|e| AiError::Provider(format!("plugin error: {e}")))?;

        match result {
            Some(json) => {
                let analysis: ErrorAnalysis = serde_json::from_str(&json)
                    .map_err(|e| AiError::Provider(format!("invalid analysis JSON: {e}")))?;
                Ok(analysis)
            }
            None => Err(AiError::Provider(
                "plugin did not return an error analysis".to_string(),
            )),
        }
    }

    async fn is_available(&self) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(AiRequest::IsAvailable { reply: tx })
            .is_err()
        {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
