//! `minal-ai` — AI engine.
//!
//! Provides the AI provider abstraction, command completion engine,
//! and Ollama integration for local AI-powered command suggestions.

pub mod completion;
mod error;
pub mod ollama;
pub mod provider;

pub use completion::{CompletionEngine, CompletionState};
pub use error::AiError;
pub use ollama::OllamaProvider;
pub use provider::{CompletionContext, CompletionResponse};
