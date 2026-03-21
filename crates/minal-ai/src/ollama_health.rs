//! Ollama memory monitoring via the `/api/ps` endpoint.

use std::time::Duration;

use crate::error::AiError;

/// Default Ollama base URL for health checks.
const DEFAULT_BASE_URL: &str = "http://localhost:11434";
/// Timeout for health check requests.
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Response from Ollama's `/api/ps` endpoint.
#[derive(serde::Deserialize, Debug)]
struct PsResponse {
    #[serde(default)]
    models: Vec<ModelInfo>,
}

/// Individual model entry in the `/api/ps` response.
#[derive(serde::Deserialize, Debug)]
struct ModelInfo {
    #[serde(default)]
    size_vram: u64,
    #[serde(default)]
    size: u64,
}

/// Monitors Ollama's memory usage by querying the `/api/ps` endpoint.
pub struct OllamaHealthChecker {
    client: reqwest::Client,
    base_url: String,
    memory_limit_mb: u64,
}

impl OllamaHealthChecker {
    /// Creates a new health checker.
    ///
    /// # Errors
    /// Returns `AiError::Http` if the HTTP client cannot be constructed.
    pub fn new(base_url: Option<String>, memory_limit_mb: u64) -> Result<Self, AiError> {
        let client = reqwest::Client::builder()
            .timeout(HEALTH_CHECK_TIMEOUT)
            .build()
            .map_err(AiError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            memory_limit_mb,
        })
    }

    /// Check current memory usage of loaded Ollama models.
    ///
    /// Returns the total VRAM usage in megabytes.
    pub async fn check_memory_usage_mb(&self) -> Result<u64, AiError> {
        let url = format!("{}/api/ps", self.base_url);
        let response = self.client.get(&url).send().await.map_err(AiError::Http)?;

        if !response.status().is_success() {
            return Err(AiError::Provider(format!(
                "Ollama /api/ps returned status {}",
                response.status()
            )));
        }

        let ps: PsResponse = response.json().await.map_err(AiError::Http)?;
        let total_bytes: u64 = ps
            .models
            .iter()
            .map(|m| if m.size_vram > 0 { m.size_vram } else { m.size })
            .sum();

        Ok(total_bytes / (1024 * 1024))
    }

    /// Whether the current memory usage is within the configured limit.
    pub async fn is_within_limit(&self) -> Result<bool, AiError> {
        let usage_mb = self.check_memory_usage_mb().await?;
        Ok(usage_mb <= self.memory_limit_mb)
    }

    /// The configured memory limit in MB.
    pub fn memory_limit_mb(&self) -> u64 {
        self.memory_limit_mb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ps_response_empty() {
        let json = r#"{"models":[]}"#;
        let ps: PsResponse = serde_json::from_str(json).unwrap();
        assert!(ps.models.is_empty());
    }

    #[test]
    fn parse_ps_response_with_models() {
        let json = r#"{"models":[{"size_vram":4294967296,"size":4294967296},{"size_vram":2147483648,"size":2147483648}]}"#;
        let ps: PsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(ps.models.len(), 2);
        // Total VRAM: 4096 MB + 2048 MB = 6144 MB
        let total_mb: u64 = ps.models.iter().map(|m| m.size_vram / (1024 * 1024)).sum();
        assert_eq!(total_mb, 6144);
    }

    #[test]
    fn parse_ps_response_missing_fields() {
        // Defensive: missing fields should default to 0.
        let json = r#"{"models":[{}]}"#;
        let ps: PsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(ps.models[0].size_vram, 0);
        assert_eq!(ps.models[0].size, 0);
    }

    #[test]
    fn construction() {
        let checker = OllamaHealthChecker::new(None, 4096).unwrap();
        assert_eq!(checker.memory_limit_mb(), 4096);
    }
}
