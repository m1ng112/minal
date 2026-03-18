//! AI provider configuration.

use serde::{Deserialize, Serialize};

/// Supported AI provider backends.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    /// Local Ollama instance.
    #[default]
    Ollama,
    /// Anthropic Claude API.
    Anthropic,
    /// OpenAI API.
    #[serde(rename = "openai")]
    OpenAi,
}

/// Default debounce time in milliseconds for AI completion.
fn default_debounce_ms() -> u64 {
    300
}

/// Default ghost text opacity.
fn default_ghost_text_opacity() -> f32 {
    0.5
}

/// AI feature configuration.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    /// Which AI provider to use.
    pub provider: AiProvider,
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
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: AiProvider::default(),
            enabled: false,
            model: None,
            base_url: None,
            debounce_ms: default_debounce_ms(),
            ghost_text_opacity: default_ghost_text_opacity(),
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let cfg = AiConfig::default();
        assert_eq!(cfg.provider, AiProvider::Ollama);
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
        assert_eq!(cfg.provider, AiProvider::Anthropic);
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
        assert_eq!(cfg.provider, AiProvider::OpenAi);
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
            provider: AiProvider::Anthropic,
            enabled: false,
            model: Some("claude-3-haiku".to_string()),
            base_url: None,
            debounce_ms: default_debounce_ms(),
            ghost_text_opacity: default_ghost_text_opacity(),
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
    fn provider_serde_lowercase() {
        // Verify serialization produces lowercase via wrapping struct
        let cfg = AiConfig {
            provider: AiProvider::Ollama,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"ollama\""));

        let cfg = AiConfig {
            provider: AiProvider::OpenAi,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"openai\""));

        let cfg = AiConfig {
            provider: AiProvider::Anthropic,
            ..AiConfig::default()
        };
        let s = toml::to_string(&cfg).unwrap();
        assert!(s.contains("\"anthropic\""));
    }
}
