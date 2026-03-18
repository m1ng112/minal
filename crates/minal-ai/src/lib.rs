//! `minal-ai` — AI engine.
//!
//! Provides the AI provider abstraction, command completion engine,
//! chat engine, and session analyzer.

pub mod completion;
pub mod context;
mod error;
pub(crate) mod ollama;
pub mod provider;

pub use completion::CompletionEngine;
pub use context::ContextGatherer;
pub use error::AiError;
pub use ollama::OllamaProvider;
pub use provider::{AiProvider, CompletionContext};
