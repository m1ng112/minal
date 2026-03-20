//! Secure API key storage.
//!
//! Provides platform-specific credential storage with environment variable fallback.

use crate::AiError;

/// Trait for secure credential storage.
pub trait KeyStore: Send + Sync {
    /// Retrieve an API key for the given provider name.
    fn get_key(&self, provider: &str) -> Result<String, AiError>;

    /// Store an API key for the given provider name.
    fn set_key(&self, provider: &str, key: &str) -> Result<(), AiError>;

    /// Delete the stored API key for the given provider name.
    fn delete_key(&self, provider: &str) -> Result<(), AiError>;
}

/// Environment variable-based key store.
///
/// Maps provider names to env vars:
/// - "anthropic" → ANTHROPIC_API_KEY
/// - "openai" → OPENAI_API_KEY
pub struct EnvKeyStore;

impl EnvKeyStore {
    fn env_var_name(provider: &str) -> String {
        match provider {
            "anthropic" => "ANTHROPIC_API_KEY".to_string(),
            "openai" => "OPENAI_API_KEY".to_string(),
            other => format!("{}_API_KEY", other.to_uppercase()),
        }
    }
}

impl KeyStore for EnvKeyStore {
    fn get_key(&self, provider: &str) -> Result<String, AiError> {
        let var_name = Self::env_var_name(provider);
        std::env::var(&var_name).map_err(|_| {
            AiError::KeystoreError(format!(
                "Environment variable {var_name} not set. Set it or use keychain storage."
            ))
        })
    }

    fn set_key(&self, _provider: &str, _key: &str) -> Result<(), AiError> {
        Err(AiError::KeystoreError(
            "Cannot set environment variables at runtime. Set them in your shell profile."
                .to_string(),
        ))
    }

    fn delete_key(&self, _provider: &str) -> Result<(), AiError> {
        Err(AiError::KeystoreError(
            "Cannot delete environment variables at runtime.".to_string(),
        ))
    }
}

/// macOS Keychain-based key store.
#[cfg(target_os = "macos")]
pub struct KeychainStore;

#[cfg(target_os = "macos")]
impl KeychainStore {
    const SERVICE_NAME: &'static str = "com.minal.ai";
}

#[cfg(target_os = "macos")]
impl KeyStore for KeychainStore {
    fn get_key(&self, provider: &str) -> Result<String, AiError> {
        // Try keychain first.
        match security_framework::passwords::get_generic_password(Self::SERVICE_NAME, provider) {
            Ok(bytes) => String::from_utf8(bytes.to_vec()).map_err(|e| {
                AiError::KeystoreError(format!("Invalid UTF-8 in keychain entry: {e}"))
            }),
            Err(_) => {
                // Fall back to environment variable.
                tracing::debug!(
                    provider,
                    "Key not found in keychain, trying environment variable"
                );
                EnvKeyStore.get_key(provider)
            }
        }
    }

    fn set_key(&self, provider: &str, key: &str) -> Result<(), AiError> {
        security_framework::passwords::set_generic_password(
            Self::SERVICE_NAME,
            provider,
            key.as_bytes(),
        )
        .map_err(|e| AiError::KeystoreError(format!("Failed to save to keychain: {e}")))
    }

    fn delete_key(&self, provider: &str) -> Result<(), AiError> {
        security_framework::passwords::delete_generic_password(Self::SERVICE_NAME, provider)
            .map_err(|e| AiError::KeystoreError(format!("Failed to delete from keychain: {e}")))
    }
}

/// Create the default key store for the current platform and configuration.
pub fn default_keystore(config: &minal_config::AiConfig) -> Box<dyn KeyStore> {
    use minal_config::ApiKeySource;
    match config.api_key_source {
        ApiKeySource::Environment => Box::new(EnvKeyStore),
        ApiKeySource::Keychain => {
            #[cfg(target_os = "macos")]
            {
                Box::new(KeychainStore)
            }
            #[cfg(not(target_os = "macos"))]
            {
                tracing::warn!(
                    "Keychain not available on this platform, falling back to environment variables"
                );
                Box::new(EnvKeyStore)
            }
        }
    }
}

/// In-memory key store for use in tests.
#[cfg(test)]
pub(crate) struct MockKeyStore {
    keys: std::collections::HashMap<String, String>,
}

#[cfg(test)]
impl MockKeyStore {
    pub(crate) fn new() -> Self {
        Self {
            keys: std::collections::HashMap::new(),
        }
    }

    pub(crate) fn with_key(mut self, provider: &str, key: &str) -> Self {
        self.keys.insert(provider.to_string(), key.to_string());
        self
    }
}

#[cfg(test)]
impl KeyStore for MockKeyStore {
    fn get_key(&self, provider: &str) -> Result<String, AiError> {
        self.keys
            .get(provider)
            .cloned()
            .ok_or_else(|| AiError::KeystoreError(format!("No key for {provider}")))
    }

    fn set_key(&self, _provider: &str, _key: &str) -> Result<(), AiError> {
        Ok(())
    }

    fn delete_key(&self, _provider: &str) -> Result<(), AiError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_key_store_missing_key() {
        let store = MockKeyStore::new();
        let result = store.get_key("anthropic");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AiError::KeystoreError(_)));
    }

    #[test]
    fn mock_key_store_present_key() {
        let store = MockKeyStore::new().with_key("anthropic", "test-key-value");
        let result = store.get_key("anthropic");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-key-value");
    }

    #[test]
    fn mock_key_store_set_and_delete_ok() {
        let store = MockKeyStore::new();
        assert!(store.set_key("anthropic", "some-key").is_ok());
        assert!(store.delete_key("anthropic").is_ok());
    }

    #[test]
    fn env_key_store_missing_var() {
        // Use a provider name that is guaranteed to have no env var set in the
        // test environment without requiring env mutation.
        let store = EnvKeyStore;
        // Pick a highly improbable env var name to avoid collisions.
        let result = store.get_key("minal_test_provider_zzzzzz_unlikely");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AiError::KeystoreError(_)));
    }

    #[test]
    fn env_key_store_set_returns_error() {
        let store = EnvKeyStore;
        let result = store.set_key("anthropic", "some-key");
        assert!(result.is_err());
    }

    #[test]
    fn env_key_store_delete_returns_error() {
        let store = EnvKeyStore;
        let result = store.delete_key("anthropic");
        assert!(result.is_err());
    }

    #[test]
    fn env_var_name_mapping() {
        assert_eq!(EnvKeyStore::env_var_name("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(EnvKeyStore::env_var_name("openai"), "OPENAI_API_KEY");
        assert_eq!(EnvKeyStore::env_var_name("custom"), "CUSTOM_API_KEY");
    }

    #[test]
    fn default_keystore_environment_uses_env_key_store() {
        let config = minal_config::AiConfig {
            api_key_source: minal_config::ApiKeySource::Environment,
            ..minal_config::AiConfig::default()
        };
        // Verify the keystore returns a keystore error for a missing provider.
        let store = default_keystore(&config);
        let result = store.get_key("minal_test_provider_zzzzzz_unlikely");
        assert!(result.is_err());
    }
}
