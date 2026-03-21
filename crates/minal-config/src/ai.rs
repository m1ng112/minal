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
    }
}
