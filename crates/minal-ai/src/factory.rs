//! Provider factory for config-driven provider instantiation.

use std::sync::Arc;

use minal_config::{AiConfig, AiProviderKind};

use crate::anthropic::AnthropicProvider;
use crate::error::AiError;
use crate::keystore::KeyStore;
use crate::ollama::OllamaProvider;
use crate::openai::OpenAiProvider;
use crate::provider::AiProvider;

/// Create an AI provider based on configuration.
///
/// # Errors
/// Returns `AiError` if the provider cannot be created (e.g., missing API key).
pub fn create_provider(
    config: &AiConfig,
    keystore: &dyn KeyStore,
) -> Result<Arc<dyn AiProvider>, AiError> {
    match config.provider {
        AiProviderKind::Ollama => {
            let provider = OllamaProvider::new(config.base_url.clone(), config.model.clone())?;
            Ok(Arc::new(provider))
        }
        AiProviderKind::Anthropic => {
            let api_key = keystore.get_key("anthropic")?;
            let provider =
                AnthropicProvider::new(api_key, config.base_url.clone(), config.model.clone())?;
            Ok(Arc::new(provider))
        }
        AiProviderKind::OpenAi => {
            let api_key = keystore.get_key("openai")?;
            let provider =
                OpenAiProvider::new(api_key, config.base_url.clone(), config.model.clone())?;
            Ok(Arc::new(provider))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystore::MockKeyStore;

    #[test]
    fn create_ollama_provider() {
        let config = AiConfig {
            provider: AiProviderKind::Ollama,
            enabled: true,
            ..AiConfig::default()
        };
        // Ollama requires no API key.
        let keystore = MockKeyStore::new();
        let provider = create_provider(&config, &keystore);
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap().name(), "ollama");
    }

    #[test]
    fn create_anthropic_with_key_succeeds() {
        let config = AiConfig {
            provider: AiProviderKind::Anthropic,
            enabled: true,
            ..AiConfig::default()
        };
        let keystore = MockKeyStore::new().with_key("anthropic", "sk-ant-test");
        let result = create_provider(&config, &keystore);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "anthropic");
    }

    #[test]
    fn create_anthropic_without_key_fails() {
        let config = AiConfig {
            provider: AiProviderKind::Anthropic,
            enabled: true,
            ..AiConfig::default()
        };
        // No key provided for "anthropic".
        let keystore = MockKeyStore::new();
        let result = create_provider(&config, &keystore);
        assert!(result.is_err());
    }

    #[test]
    fn create_openai_with_key_succeeds() {
        let config = AiConfig {
            provider: AiProviderKind::OpenAi,
            enabled: true,
            ..AiConfig::default()
        };
        let keystore = MockKeyStore::new().with_key("openai", "sk-test");
        let result = create_provider(&config, &keystore);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name(), "openai");
    }

    #[test]
    fn create_openai_without_key_fails() {
        let config = AiConfig {
            provider: AiProviderKind::OpenAi,
            enabled: true,
            ..AiConfig::default()
        };
        // No key provided for "openai".
        let keystore = MockKeyStore::new();
        let result = create_provider(&config, &keystore);
        assert!(result.is_err());
    }
}
