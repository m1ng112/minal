//! AI provider configuration.

use serde::{Deserialize, Serialize};

/// Supported AI provider backends.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderKind {
    /// Local Ollama instance.
    #[default]
    Ollama,
    /// Anthropic Claude API.
    Anthropic,
    /// OpenAI API.
    #[serde(rename = "openai")]
    OpenAi,
    /// A WASM plugin-provided AI backend.
    /// Use the `plugin_provider` field in `AiConfig` to specify the plugin name.
    Plugin,
}

/// How to retrieve the API key for cloud providers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeySource {
    /// Read from system keychain (macOS Keychain, libsecret on Linux).
    #[default]
    Keychain,
    /// Read from environment variable.
    Environment,
}

/// Default debounce time in milliseconds for AI completion.
fn default_debounce_ms() -> u64 {
    300
}

/// Default completion cache size (LRU entries).
fn default_completion_cache_size() -> usize {
    256
}

/// Default completion timeout in milliseconds.
fn default_completion_timeout_ms() -> u64 {
    2000
}

/// Default ghost text opacity.
fn default_ghost_text_opacity() -> f32 {
    0.5
}

/// Helper for serde `default = "default_true"`.
fn default_true() -> bool {
    true
}

/// Default maximum output characters for privacy truncation.
fn default_max_output_chars() -> usize {
    2000
}

/// Default maximum command history entries.
fn default_max_command_history() -> usize {
    20
}

/// Default panel height ratio for inline chat.
fn default_panel_height_ratio() -> f32 {
    0.3
}

/// Default maximum chat history messages.
fn default_max_chat_history() -> usize {
    50
}

/// Default maximum number of session analysis errors to retain.
fn default_max_analysis_errors() -> usize {
    50
}

/// Privacy settings for AI context collection.
///
/// Controls what information is sent to the AI provider.
/// The `[ai.privacy]` section in the TOML configuration.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AiPrivacyConfig {
    /// Glob patterns to exclude from context (e.g., `["*.env", "credentials*"]`).
    pub exclude_patterns: Vec<String>,
    /// Whether to send the current working directory.
    pub send_cwd: bool,
    /// Whether to send git status information.
    pub send_git_status: bool,
    /// Whether to send environment variable hints.
    pub send_env: bool,
    /// Maximum characters of terminal output to include.
    #[serde(default = "default_max_output_chars")]
    pub max_output_chars: usize,
    /// Maximum number of command history entries to include.
    #[serde(default = "default_max_command_history")]
    pub max_command_history: usize,
}

impl Default for AiPrivacyConfig {
    fn default() -> Self {
        Self {
            exclude_patterns: vec!["*.env".to_string(), "credentials*".to_string()],
            send_cwd: true,
            send_git_status: true,
            send_env: false,
            max_output_chars: default_max_output_chars(),
            max_command_history: default_max_command_history(),
        }
    }
}

impl AiPrivacyConfig {
    /// Validates the privacy configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        if self.max_output_chars == 0 {
            return Err(super::ConfigError::Validation(
                "ai.privacy.max_output_chars must be > 0".to_string(),
            ));
        }
        if self.max_command_history == 0 {
            return Err(super::ConfigError::Validation(
                "ai.privacy.max_command_history must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Chat panel configuration.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ChatConfig {
    /// Panel height as fraction of window height (0.1 to 0.8).
    #[serde(default = "default_panel_height_ratio")]
    pub panel_height_ratio: f32,
    /// Maximum number of conversation messages to retain.
    #[serde(default = "default_max_chat_history")]
    pub max_history: usize,
    /// Optional system prompt for the chat engine.
    pub system_prompt: Option<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            panel_height_ratio: default_panel_height_ratio(),
            max_history: default_max_chat_history(),
            system_prompt: None,
        }
    }
}

impl ChatConfig {
    /// Validates the chat configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        if !(0.1..=0.8).contains(&self.panel_height_ratio) {
            return Err(super::ConfigError::Validation(format!(
                "ai.chat.panel_height_ratio must be between 0.1 and 0.8, got {}",
                self.panel_height_ratio
            )));
        }
        if self.max_history == 0 {
            return Err(super::ConfigError::Validation(
                "ai.chat.max_history must be > 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Session analysis configuration.
///
/// Controls automatic error detection and AI analysis of terminal command failures.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct SessionAnalysisConfig {
    /// Whether session analysis is enabled.
    pub enabled: bool,
    /// Whether to automatically request AI analysis on error detection.
    pub auto_ai_analysis: bool,
    /// Maximum number of errors to retain.
    #[serde(default = "default_max_analysis_errors")]
    pub max_errors: usize,
}

impl Default for SessionAnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_ai_analysis: true,
            max_errors: default_max_analysis_errors(),
        }
    }
}

impl SessionAnalysisConfig {
    /// Validates the session analysis configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        if self.max_errors == 0 || self.max_errors > 200 {
            return Err(super::ConfigError::Validation(format!(
                "ai.session_analysis.max_errors must be between 1 and 200, got {}",
                self.max_errors
            )));
        }
        Ok(())
    }
}

/// Default maximum agent steps.
fn default_max_agent_steps() -> usize {
    20
}

/// Default agent step timeout in seconds.
fn default_step_timeout_secs() -> u64 {
    300
}

/// Default agent panel height ratio.
fn default_agent_panel_height_ratio() -> f32 {
    0.4
}

/// Agent approval mode.
///
/// Controls how agent actions are approved before execution.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    /// Every step requires explicit user approval (default).
    #[default]
    Step,
    /// Auto-approve safe commands; dangerous ones still require approval.
    #[serde(rename = "auto_safe")]
    AutoSafe,
    /// Auto-approve all commands (dangerous commands show warning but execute).
    #[serde(rename = "auto_all")]
    AutoAll,
}

/// Agent mode configuration.
///
/// Controls the autonomous AI agent that can plan and execute tasks.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AgentConfig {
    /// Whether agent mode is enabled.
    pub enabled: bool,
    /// How actions are approved.
    pub approval_mode: ApprovalMode,
    /// Maximum number of steps in a single agent task.
    #[serde(default = "default_max_agent_steps")]
    pub max_steps: usize,
    /// Timeout for a single step in seconds.
    #[serde(default = "default_step_timeout_secs")]
    pub step_timeout_secs: u64,
    /// Panel height as fraction of window height.
    #[serde(default = "default_agent_panel_height_ratio")]
    pub panel_height_ratio: f32,
    /// Patterns for dangerous commands that require extra warnings.
    pub dangerous_commands: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            approval_mode: ApprovalMode::default(),
            max_steps: default_max_agent_steps(),
            step_timeout_secs: default_step_timeout_secs(),
            panel_height_ratio: default_agent_panel_height_ratio(),
            dangerous_commands: vec![
                "rm -rf".to_string(),
                "sudo".to_string(),
                "dd ".to_string(),
                "mkfs".to_string(),
                "chmod -R 777".to_string(),
                "> /dev/".to_string(),
                "format ".to_string(),
            ],
        }
    }
}

impl AgentConfig {
    /// Validates the agent configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        if self.max_steps == 0 || self.max_steps > 100 {
            return Err(super::ConfigError::Validation(format!(
                "ai.agent.max_steps must be between 1 and 100, got {}",
                self.max_steps
            )));
        }
        if self.step_timeout_secs < 10 || self.step_timeout_secs > 3600 {
            return Err(super::ConfigError::Validation(format!(
                "ai.agent.step_timeout_secs must be between 10 and 3600, got {}",
                self.step_timeout_secs
            )));
        }
        if !(0.1..=0.8).contains(&self.panel_height_ratio) {
            return Err(super::ConfigError::Validation(format!(
                "ai.agent.panel_height_ratio must be between 0.1 and 0.8, got {}",
                self.panel_height_ratio
            )));
        }
        Ok(())
    }
}

/// AI feature configuration.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    /// Which AI provider to use.
    pub provider: AiProviderKind,
    /// Whether AI features are enabled.
    pub enabled: bool,
    /// Model name to use (provider-specific). `None` uses provider default.
    pub model: Option<String>,
    /// Custom base URL for the AI provider API. `None` uses provider default.
    pub base_url: Option<String>,
    /// Debounce time in milliseconds before requesting AI completion.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    /// Opacity of ghost text completion overlay (0.0 to 1.0).
    #[serde(default = "default_ghost_text_opacity")]
    pub ghost_text_opacity: f32,
    /// How to retrieve the API key for cloud providers. Ignored for Ollama.
    #[serde(default)]
    pub api_key_source: ApiKeySource,
    /// Privacy settings for AI context collection.
    #[serde(default)]
    pub privacy: AiPrivacyConfig,
    /// Maximum number of LRU cache entries for completion results.
    #[serde(default = "default_completion_cache_size")]
    pub completion_cache_size: usize,
    /// Timeout in milliseconds for a single completion request.
    #[serde(default = "default_completion_timeout_ms")]
    pub completion_timeout_ms: u64,
    /// Whether to send a warmup request to Ollama on startup.
    #[serde(default = "default_true")]
    pub ollama_warmup: bool,
    /// Optional memory limit (MB) for Ollama process monitoring.
    /// `None` disables monitoring.
    #[serde(default)]
    pub ollama_memory_limit_mb: Option<u64>,
    /// Fallback provider when the primary is unavailable.
    /// `None` disables fallback.
    #[serde(default)]
    pub fallback_provider: Option<AiProviderKind>,
    /// Chat panel settings.
    #[serde(default)]
    pub chat: ChatConfig,
    /// Session analysis settings.
    #[serde(default)]
    pub session_analysis: SessionAnalysisConfig,
    /// Agent mode settings.
    #[serde(default)]
    pub agent: AgentConfig,
    /// Name of the plugin to use as AI provider (when `provider = "plugin"`).
    #[serde(default)]
    pub plugin_provider: Option<String>,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: AiProviderKind::default(),
            enabled: false,
            model: None,
            base_url: None,
            debounce_ms: default_debounce_ms(),
            ghost_text_opacity: default_ghost_text_opacity(),
            api_key_source: ApiKeySource::default(),
            privacy: AiPrivacyConfig::default(),
            completion_cache_size: default_completion_cache_size(),
            completion_timeout_ms: default_completion_timeout_ms(),
            ollama_warmup: true,
            ollama_memory_limit_mb: None,
            fallback_provider: None,
            chat: ChatConfig::default(),
            session_analysis: SessionAnalysisConfig::default(),
            agent: AgentConfig::default(),
            plugin_provider: None,
        }
    }
}

impl AiConfig {
    /// Validates the AI configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        if !(50..=2000).contains(&self.debounce_ms) {
            return Err(super::ConfigError::Validation(format!(
                "ai.debounce_ms must be between 50 and 2000, got {}",
                self.debounce_ms
            )));
        }
        if !(0.0..=1.0).contains(&self.ghost_text_opacity) {
            return Err(super::ConfigError::Validation(format!(
                "ai.ghost_text_opacity must be between 0.0 and 1.0, got {}",
                self.ghost_text_opacity
            )));
        }
        if self.completion_cache_size > 4096 {
            return Err(super::ConfigError::Validation(format!(
                "ai.completion_cache_size must be <= 4096, got {}",
                self.completion_cache_size
            )));
        }
        if !(500..=30000).contains(&self.completion_timeout_ms) {
            return Err(super::ConfigError::Validation(format!(
                "ai.completion_timeout_ms must be between 500 and 30000, got {}",
                self.completion_timeout_ms
            )));
        }
        self.privacy.validate()?;
        self.chat.validate()?;
        self.session_analysis.validate()?;
        self.agent.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let cfg = AiConfig::default();
        assert_eq!(cfg.provider, AiProviderKind::Ollama);
        assert!(!cfg.enabled);
        assert_eq!(cfg.model, None);
        assert_eq!(cfg.base_url, None);
    }

    #[test]
    fn deserialize_full() {
        let toml_str = r#"
            provider = "anthropic"
            enabled = false
            model = "claude-3-haiku"
            base_url = "https://api.anthropic.com"
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.provider, AiProviderKind::Anthropic);
        assert!(!cfg.enabled);
        assert_eq!(cfg.model, Some("claude-3-haiku".to_string()));
        assert_eq!(cfg.base_url, Some("https://api.anthropic.com".to_string()));
    }

    #[test]
    fn deserialize_partial() {
        let toml_str = r#"
            provider = "openai"
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.provider, AiProviderKind::OpenAi);
        assert!(!cfg.enabled);
        assert_eq!(cfg.model, None);
    }

    #[test]
    fn deserialize_empty() {
        let cfg: AiConfig = toml::from_str("").unwrap();
        assert_eq!(cfg, AiConfig::default());
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = AiConfig {
            provider: AiProviderKind::Anthropic,
            enabled: false,
            model: Some("claude-3-haiku".to_string()),
            base_url: None,
            debounce_ms: default_debounce_ms(),
            ghost_text_opacity: default_ghost_text_opacity(),
            api_key_source: ApiKeySource::Environment,
            privacy: AiPrivacyConfig::default(),
            completion_cache_size: 128,
            completion_timeout_ms: 3000,
            ollama_warmup: false,
            ollama_memory_limit_mb: Some(4096),
            fallback_provider: Some(AiProviderKind::Ollama),
            chat: ChatConfig::default(),
            session_analysis: SessionAnalysisConfig::default(),
            agent: AgentConfig::default(),
            plugin_provider: None,
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: AiConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn serialize_roundtrip_plugin_provider() {
        let cfg = AiConfig {
            provider: AiProviderKind::Plugin,
            plugin_provider: Some("my-ai-plugin".to_string()),
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: AiConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn validate_debounce_ms() {
        let mut cfg = AiConfig::default();
        cfg.debounce_ms = 10;
        assert!(cfg.validate().is_err());
        cfg.debounce_ms = 3000;
        assert!(cfg.validate().is_err());
        cfg.debounce_ms = 300;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_ghost_text_opacity() {
        let mut cfg = AiConfig::default();
        cfg.ghost_text_opacity = -0.1;
        assert!(cfg.validate().is_err());
        cfg.ghost_text_opacity = 1.1;
        assert!(cfg.validate().is_err());
        cfg.ghost_text_opacity = 0.5;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn privacy_default_values() {
        let cfg = AiPrivacyConfig::default();
        assert!(cfg.send_cwd);
        assert!(cfg.send_git_status);
        assert!(!cfg.send_env);
        assert_eq!(cfg.max_output_chars, 2000);
        assert_eq!(cfg.max_command_history, 20);
        assert_eq!(cfg.exclude_patterns, vec!["*.env", "credentials*"]);
    }

    #[test]
    fn privacy_deserialize_from_toml() {
        let toml_str = r#"
            provider = "anthropic"
            enabled = true

            [privacy]
            exclude_patterns = ["*.env", "*.pem"]
            send_cwd = true
            send_git_status = false
            send_env = false
            max_output_chars = 1000
            max_command_history = 10
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert!(!cfg.privacy.send_git_status);
        assert_eq!(cfg.privacy.max_output_chars, 1000);
        assert_eq!(cfg.privacy.max_command_history, 10);
        assert_eq!(cfg.privacy.exclude_patterns, vec!["*.env", "*.pem"]);
    }

    #[test]
    fn privacy_missing_uses_defaults() {
        let toml_str = r#"
            provider = "anthropic"
            enabled = true
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.privacy, AiPrivacyConfig::default());
    }

    #[test]
    fn privacy_partial_uses_defaults_for_missing() {
        let toml_str = r#"
            provider = "anthropic"
            [privacy]
            send_env = true
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.privacy.send_env);
        // Other fields should be defaults
        assert!(cfg.privacy.send_cwd);
        assert!(cfg.privacy.send_git_status);
        assert_eq!(cfg.privacy.max_output_chars, 2000);
    }

    #[test]
    fn privacy_validate_zero_output_chars() {
        let mut cfg = AiPrivacyConfig::default();
        cfg.max_output_chars = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn privacy_validate_zero_command_history() {
        let mut cfg = AiPrivacyConfig::default();
        cfg.max_command_history = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_completion_cache_size() {
        let mut cfg = AiConfig::default();
        cfg.completion_cache_size = 5000;
        assert!(cfg.validate().is_err());
        cfg.completion_cache_size = 256;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_completion_timeout_ms() {
        let mut cfg = AiConfig::default();
        cfg.completion_timeout_ms = 100;
        assert!(cfg.validate().is_err());
        cfg.completion_timeout_ms = 50000;
        assert!(cfg.validate().is_err());
        cfg.completion_timeout_ms = 2000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn deserialize_new_fields() {
        let toml_str = r#"
            provider = "anthropic"
            enabled = true
            completion_cache_size = 128
            completion_timeout_ms = 3000
            ollama_warmup = false
            ollama_memory_limit_mb = 4096
            fallback_provider = "ollama"
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.completion_cache_size, 128);
        assert_eq!(cfg.completion_timeout_ms, 3000);
        assert!(!cfg.ollama_warmup);
        assert_eq!(cfg.ollama_memory_limit_mb, Some(4096));
        assert_eq!(cfg.fallback_provider, Some(AiProviderKind::Ollama));
    }

    #[test]
    fn new_fields_default_when_missing() {
        let cfg: AiConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.completion_cache_size, 256);
        assert_eq!(cfg.completion_timeout_ms, 2000);
        assert!(cfg.ollama_warmup);
        assert_eq!(cfg.ollama_memory_limit_mb, None);
        assert_eq!(cfg.fallback_provider, None);
    }

    #[test]
    fn provider_serde_lowercase() {
        // Verify serialization produces lowercase via wrapping struct
        let cfg = AiConfig {
            provider: AiProviderKind::Ollama,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"ollama\""));

        let cfg = AiConfig {
            provider: AiProviderKind::OpenAi,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"openai\""));

        let cfg = AiConfig {
            provider: AiProviderKind::Anthropic,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"anthropic\""));

        let cfg = AiConfig {
            provider: AiProviderKind::Plugin,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"plugin\""));
    }

    #[test]
    fn chat_default_values() {
        let cfg = ChatConfig::default();
        assert!((cfg.panel_height_ratio - 0.3).abs() < f32::EPSILON);
        assert_eq!(cfg.max_history, 50);
        assert_eq!(cfg.system_prompt, None);
    }

    #[test]
    fn chat_serialize_roundtrip() {
        let cfg = ChatConfig {
            panel_height_ratio: 0.5,
            max_history: 100,
            system_prompt: Some("You are a helpful assistant.".to_string()),
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: ChatConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn chat_validate_panel_height_ratio_too_low() {
        let mut cfg = ChatConfig::default();
        cfg.panel_height_ratio = 0.05;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn chat_validate_panel_height_ratio_too_high() {
        let mut cfg = ChatConfig::default();
        cfg.panel_height_ratio = 0.9;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn chat_validate_panel_height_ratio_valid() {
        let mut cfg = ChatConfig::default();
        cfg.panel_height_ratio = 0.1;
        assert!(cfg.validate().is_ok());
        cfg.panel_height_ratio = 0.8;
        assert!(cfg.validate().is_ok());
        cfg.panel_height_ratio = 0.5;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn chat_validate_max_history_zero() {
        let mut cfg = ChatConfig::default();
        cfg.max_history = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn chat_partial_toml_uses_defaults() {
        let toml_str = r#"
            provider = "ollama"
            [chat]
            max_history = 100
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.chat.max_history, 100);
        // Other chat fields should be defaults
        assert!((cfg.chat.panel_height_ratio - 0.3).abs() < f32::EPSILON);
        assert_eq!(cfg.chat.system_prompt, None);
    }

    #[test]
    fn chat_missing_section_uses_defaults() {
        let toml_str = r#"
            provider = "ollama"
            enabled = true
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.chat, ChatConfig::default());
    }

    #[test]
    fn session_analysis_default_values() {
        let cfg = SessionAnalysisConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.auto_ai_analysis);
        assert_eq!(cfg.max_errors, 50);
    }

    #[test]
    fn session_analysis_validate_zero_max_errors() {
        let mut cfg = SessionAnalysisConfig::default();
        cfg.max_errors = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn session_analysis_validate_max_errors_too_high() {
        let mut cfg = SessionAnalysisConfig::default();
        cfg.max_errors = 201;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn session_analysis_validate_valid() {
        let cfg = SessionAnalysisConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn session_analysis_partial_toml_uses_defaults() {
        let toml_str = r#"
            provider = "ollama"
            [session_analysis]
            enabled = false
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert!(!cfg.session_analysis.enabled);
        assert!(cfg.session_analysis.auto_ai_analysis);
        assert_eq!(cfg.session_analysis.max_errors, 50);
    }

    #[test]
    fn session_analysis_missing_uses_defaults() {
        let toml_str = r#"
            provider = "ollama"
            enabled = true
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.session_analysis, SessionAnalysisConfig::default());
    }

    #[test]
    fn agent_default_values() {
        let cfg = AgentConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.approval_mode, ApprovalMode::Step);
        assert_eq!(cfg.max_steps, 20);
        assert_eq!(cfg.step_timeout_secs, 300);
        assert!(!cfg.dangerous_commands.is_empty());
    }

    #[test]
    fn agent_validate_max_steps_zero() {
        let mut cfg = AgentConfig::default();
        cfg.max_steps = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn agent_validate_max_steps_too_high() {
        let mut cfg = AgentConfig::default();
        cfg.max_steps = 101;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn agent_validate_timeout_too_low() {
        let mut cfg = AgentConfig::default();
        cfg.step_timeout_secs = 5;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn agent_validate_valid() {
        let cfg = AgentConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn agent_approval_mode_serde_roundtrip() {
        let cfg = AgentConfig {
            approval_mode: ApprovalMode::AutoSafe,
            ..AgentConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: AgentConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn agent_partial_toml_uses_defaults() {
        let toml_str = r#"
            provider = "ollama"
            [agent]
            enabled = false
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert!(!cfg.agent.enabled);
        assert_eq!(cfg.agent.max_steps, 20);
    }

    #[test]
    fn agent_missing_uses_defaults() {
        let toml_str = r#"
            provider = "ollama"
            enabled = true
        "#;
        let cfg: AiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.agent, AgentConfig::default());
    }
}
