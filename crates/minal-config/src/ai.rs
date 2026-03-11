//! AI provider configuration.

use serde::Deserialize;

/// AI integration settings.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Whether AI features are enabled.
    pub enabled: bool,
    /// AI provider name (e.g. "ollama", "anthropic").
    pub provider: String,
    /// Model name to use.
    pub model: String,
    /// Optional custom endpoint URL.
    pub endpoint: Option<String>,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: "ollama".to_string(),
            model: "codellama".to_string(),
            endpoint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let ai = AiConfig::default();
        assert!(ai.enabled);
        assert_eq!(ai.provider, "ollama");
        assert_eq!(ai.model, "codellama");
        assert!(ai.endpoint.is_none());
    }

    #[test]
    fn test_partial_override() {
        let toml_str = r#"
provider = "anthropic"
model = "claude-3-haiku"
"#;
        let ai: AiConfig = toml::from_str(toml_str).unwrap();
        assert!(ai.enabled); // default true
        assert_eq!(ai.provider, "anthropic");
    }
}
