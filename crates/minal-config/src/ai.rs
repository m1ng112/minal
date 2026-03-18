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

/// AI feature configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
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
}

impl AiConfig {
    /// Validates the AI configuration.
    ///
    /// Currently a no-op; reserved for future validation
    /// (e.g. `base_url` format checking).
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
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
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: AiConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
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
