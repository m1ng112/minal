//! Provider factory for config-driven provider instantiation.

use std::sync::Arc;
use std::time::Duration;

use minal_config::{AiConfig, AiProviderKind};

use crate::anthropic::AnthropicProvider;
use crate::error::AiError;
use crate::fallback::FallbackProvider;
use crate::keystore::KeyStore;
use crate::ollama::OllamaProvider;
use crate::openai::OpenAiProvider;
use crate::provider::AiProvider;

/// Create a single provider for the given kind (without fallback wrapping).
fn create_single_provider(
    kind: &AiProviderKind,
    config: &AiConfig,
    keystore: &dyn KeyStore,
) -> Result<Arc<dyn AiProvider>, AiError> {
    match kind {
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
        AiProviderKind::Plugin => Err(AiError::Provider(
            "plugin AI providers are created via PluginManager, not the factory".to_string(),
        )),
    }
}

/// Create an AI provider based on configuration.
///
/// If a fallback provider is configured and differs from the primary,
/// the returned provider is wrapped in a [`FallbackProvider`] that
/// automatically fails over on transient errors and enforces the
/// configured completion timeout.
///
/// # Errors
/// Returns `AiError` if the primary provider cannot be created.
pub fn create_provider(
    config: &AiConfig,
    keystore: &dyn KeyStore,
) -> Result<Arc<dyn AiProvider>, AiError> {
    let primary = create_single_provider(&config.provider, config, keystore)?;

    // Wrap with fallback if configured and different from primary.
    let timeout = Duration::from_millis(config.completion_timeout_ms);
    if let Some(ref fallback_kind) = config.fallback_provider {
        if fallback_kind != &config.provider {
            let fallback = match create_single_provider(fallback_kind, config, keystore) {
                Ok(fb) => {
                    tracing::info!(
                        primary = primary.name(),
                        fallback = fb.name(),
                        "Fallback provider configured"
                    );
                    Some(fb)
                }
                Err(e) => {
                    tracing::warn!(
                        fallback = ?fallback_kind,
                        error = %e,
                        "Failed to create fallback provider; continuing without fallback"
                    );
                    None
                }
            };
            return Ok(Arc::new(FallbackProvider::new(primary, fallback, timeout)));
        }
    }

    // No fallback — still wrap for timeout enforcement.
    Ok(Arc::new(FallbackProvider::new(primary, None, timeout)))
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
        let keystore = MockKeyStore::new();
        let provider = create_provider(&config, &keystore);
        assert!(provider.is_ok());
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
    }

    #[test]
    fn create_anthropic_without_key_fails() {
        let config = AiConfig {
            provider: AiProviderKind::Anthropic,
            enabled: true,
            ..AiConfig::default()
        };
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
    }

    #[test]
    fn create_openai_without_key_fails() {
        let config = AiConfig {
            provider: AiProviderKind::OpenAi,
            enabled: true,
            ..AiConfig::default()
        };
        let keystore = MockKeyStore::new();
        let result = create_provider(&config, &keystore);
        assert!(result.is_err());
    }

    #[test]
    fn fallback_provider_created_when_configured() {
        let config = AiConfig {
            provider: AiProviderKind::Anthropic,
            enabled: true,
            fallback_provider: Some(AiProviderKind::Ollama),
            ..AiConfig::default()
        };
        let keystore = MockKeyStore::new().with_key("anthropic", "sk-ant-test");
        let result = create_provider(&config, &keystore);
        assert!(result.is_ok());
    }

    #[test]
    fn fallback_same_as_primary_no_wrap() {
        let config = AiConfig {
            provider: AiProviderKind::Ollama,
            enabled: true,
            fallback_provider: Some(AiProviderKind::Ollama),
            ..AiConfig::default()
        };
        let keystore = MockKeyStore::new();
        let result = create_provider(&config, &keystore);
        assert!(result.is_ok());
    }
}
