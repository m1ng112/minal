//! `minal-ai` — AI engine.
//!
//! Provides the AI provider abstraction, command completion engine,
//! chat engine, and session analyzer.

pub(crate) mod anthropic;
pub mod completion;
pub mod context;
mod error;
pub mod factory;
pub mod keystore;
pub(crate) mod ollama;
pub(crate) mod openai;
pub mod provider;
pub mod types;

pub use anthropic::AnthropicProvider;
pub use completion::CompletionEngine;
pub use context::ContextGatherer;
pub use error::AiError;
pub use factory::create_provider;
pub use keystore::{KeyStore, default_keystore};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use provider::AiProvider;
pub use types::{AiContext, ErrorAnalysis, ErrorContext, Message, Role};

/// Backward-compatible alias for [`AiContext`].
///
/// Deprecated: use [`AiContext`] directly.
pub type CompletionContext = AiContext;
