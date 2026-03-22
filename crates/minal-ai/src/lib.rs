//! `minal-ai` — AI engine.
//!
//! Provides the AI provider abstraction, command completion engine,
//! chat engine, and session analyzer.

pub mod agent;
pub mod analyzer;
pub(crate) mod anthropic;
pub mod cache;
pub mod chat;
pub mod completion;
pub mod context;
mod error;
pub mod factory;
pub mod fallback;
pub mod keystore;
pub(crate) mod ollama;
pub mod ollama_health;
pub(crate) mod openai;
pub mod provider;
pub(crate) mod provider_status;
pub mod types;

pub use agent::{
    AgentAction, AgentEngine, AgentPlan, AgentState, AgentStep, DangerLevel,
    DangerousCommandChecker, StepResult, StepStatus,
};
pub use analyzer::SessionAnalyzer;
pub use anthropic::AnthropicProvider;
pub use cache::CompletionCache;
pub use chat::ChatEngine;
pub use completion::CompletionEngine;
pub use context::{ContextCollector, ContextGatherer};
pub use error::AiError;
pub use factory::create_provider;
pub use fallback::FallbackProvider;
pub use keystore::{KeyStore, default_keystore};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use provider::AiProvider;
pub use types::{
    AiContext, CommandRecord, DetectedError, ErrorAnalysis, ErrorCategory, ErrorContext, GitInfo,
    Message, ProjectType, Role,
};

/// Backward-compatible alias for [`AiContext`].
///
/// Deprecated: use [`AiContext`] directly.
pub type CompletionContext = AiContext;
