//! Fallback provider wrapper with timeout and automatic failover.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::Mutex;

use crate::error::AiError;
use crate::provider::AiProvider;
use crate::types::{AiContext, ErrorAnalysis, ErrorContext, Message};

/// Interval between re-checks of a failed primary provider.
const RECHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Wraps a primary provider with an optional fallback.
///
/// When the primary fails (network error, timeout, unavailable), the wrapper
/// automatically tries the fallback provider if one is configured.
/// The primary is periodically re-checked after becoming unavailable.
pub struct FallbackProvider {
    primary: Arc<dyn AiProvider>,
    fallback: Option<Arc<dyn AiProvider>>,
    primary_available: AtomicBool,
    last_primary_check: Mutex<Instant>,
    completion_timeout: Duration,
}

impl FallbackProvider {
    /// Creates a new fallback-wrapped provider.
    ///
    /// `completion_timeout` is applied to each individual provider call.
    pub fn new(
        primary: Arc<dyn AiProvider>,
        fallback: Option<Arc<dyn AiProvider>>,
        completion_timeout: Duration,
    ) -> Self {
        Self {
            primary,
            fallback,
            primary_available: AtomicBool::new(true),
            last_primary_check: Mutex::new(Instant::now()),
            completion_timeout,
        }
    }

    /// Check if the primary should be retried based on the recheck interval.
    async fn should_retry_primary(&self) -> bool {
        let mut last = self.last_primary_check.lock().await;
        if last.elapsed() >= RECHECK_INTERVAL {
            *last = Instant::now();
            true
        } else {
            false
        }
    }

    fn mark_primary_unavailable(&self) {
        self.primary_available.store(false, Ordering::Relaxed);
    }

    fn mark_primary_available(&self) {
        self.primary_available.store(true, Ordering::Relaxed);
    }

    fn is_primary_available(&self) -> bool {
        self.primary_available.load(Ordering::Relaxed)
    }

    /// Returns true if the error is a transient issue that should trigger fallback.
    fn is_fallback_worthy(err: &AiError) -> bool {
        matches!(
            err,
            AiError::Http(_) | AiError::Timeout | AiError::Unavailable(_) | AiError::StreamError(_)
        )
    }
}

#[async_trait]
impl AiProvider for FallbackProvider {
    async fn complete(&self, context: &AiContext) -> Result<String, AiError> {
        // Try primary if it's available, or if enough time has passed.
        let try_primary = self.is_primary_available() || self.should_retry_primary().await;

        if try_primary {
            match tokio::time::timeout(self.completion_timeout, self.primary.complete(context))
                .await
            {
                Ok(Ok(result)) => {
                    self.mark_primary_available();
                    return Ok(result);
                }
                Ok(Err(e)) if Self::is_fallback_worthy(&e) => {
                    tracing::warn!(
                        primary = self.primary.name(),
                        error = %e,
                        "Primary provider failed, attempting fallback"
                    );
                    self.mark_primary_unavailable();
                }
                Ok(Err(e)) => {
                    // Non-transient error (auth, rate limit) — don't fallback.
                    return Err(e);
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        primary = self.primary.name(),
                        timeout_ms = self.completion_timeout.as_millis() as u64,
                        "Primary provider timed out, attempting fallback"
                    );
                    self.mark_primary_unavailable();
                }
            }
        }

        // Try fallback.
        if let Some(ref fallback) = self.fallback {
            match tokio::time::timeout(self.completion_timeout, fallback.complete(context)).await {
                Ok(result) => result,
                Err(_elapsed) => Err(AiError::Timeout),
            }
        } else {
            Err(AiError::Unavailable(format!(
                "Primary provider '{}' is unavailable and no fallback is configured",
                self.primary.name()
            )))
        }
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        context: &AiContext,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError> {
        // For chat_stream, try primary first then fallback on transient errors.
        let try_primary = self.is_primary_available() || self.should_retry_primary().await;

        if try_primary {
            match self.primary.chat_stream(messages, context).await {
                Ok(stream) => {
                    self.mark_primary_available();
                    return Ok(stream);
                }
                Err(e) if Self::is_fallback_worthy(&e) => {
                    tracing::warn!(
                        primary = self.primary.name(),
                        error = %e,
                        "Primary provider chat failed, attempting fallback"
                    );
                    self.mark_primary_unavailable();
                }
                Err(e) => return Err(e),
            }
        }

        if let Some(ref fallback) = self.fallback {
            fallback.chat_stream(messages, context).await
        } else {
            Err(AiError::Unavailable(format!(
                "Primary provider '{}' is unavailable for chat",
                self.primary.name()
            )))
        }
    }

    async fn analyze_error(&self, error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
        let try_primary = self.is_primary_available() || self.should_retry_primary().await;

        if try_primary {
            match self.primary.analyze_error(error).await {
                Ok(analysis) => {
                    self.mark_primary_available();
                    return Ok(analysis);
                }
                Err(e) if Self::is_fallback_worthy(&e) => {
                    tracing::warn!(
                        primary = self.primary.name(),
                        error = %e,
                        "Primary provider analysis failed, attempting fallback"
                    );
                    self.mark_primary_unavailable();
                }
                Err(e) => return Err(e),
            }
        }

        if let Some(ref fallback) = self.fallback {
            fallback.analyze_error(error).await
        } else {
            Err(AiError::Unavailable(format!(
                "Primary provider '{}' is unavailable for analysis",
                self.primary.name()
            )))
        }
    }

    async fn is_available(&self) -> bool {
        if self.primary.is_available().await {
            return true;
        }
        if let Some(ref fallback) = self.fallback {
            fallback.is_available().await
        } else {
            false
        }
    }

    fn name(&self) -> &str {
        // Return primary name; callers can check is_available for details.
        self.primary.name()
    }

    async fn warmup(&self) -> Result<(), AiError> {
        // Warm up both providers.
        let primary_result = self.primary.warmup().await;
        if let Some(ref fallback) = self.fallback {
            if let Err(e) = fallback.warmup().await {
                tracing::warn!(fallback = fallback.name(), "Fallback warmup failed: {e}");
            }
        }
        primary_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A mock provider for testing fallback behavior.
    struct MockProvider {
        name: &'static str,
        available: AtomicBool,
        call_count: AtomicUsize,
        fail_with: Option<fn() -> AiError>,
    }

    impl MockProvider {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                available: AtomicBool::new(true),
                call_count: AtomicUsize::new(0),
                fail_with: None,
            }
        }

        fn failing(name: &'static str, err_fn: fn() -> AiError) -> Self {
            Self {
                name,
                available: AtomicBool::new(true),
                call_count: AtomicUsize::new(0),
                fail_with: Some(err_fn),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl AiProvider for MockProvider {
        async fn complete(&self, _context: &AiContext) -> Result<String, AiError> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            if let Some(err_fn) = self.fail_with {
                Err(err_fn())
            } else {
                Ok(format!("{}_completion", self.name))
            }
        }

        async fn chat_stream(
            &self,
            _messages: &[Message],
            _context: &AiContext,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError> {
            Err(AiError::Provider("not implemented in mock".to_string()))
        }

        async fn analyze_error(&self, _error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
            Err(AiError::Provider("not implemented in mock".to_string()))
        }

        async fn is_available(&self) -> bool {
            self.available.load(Ordering::Relaxed)
        }

        fn name(&self) -> &str {
            self.name
        }
    }

    fn default_context() -> AiContext {
        AiContext {
            input_prefix: "test".to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn primary_success_no_fallback() {
        let primary = Arc::new(MockProvider::new("primary"));
        let fallback = Arc::new(MockProvider::new("fallback"));
        let provider = FallbackProvider::new(
            primary.clone(),
            Some(fallback.clone()),
            Duration::from_secs(5),
        );

        let result = provider.complete(&default_context()).await;
        assert_eq!(result.unwrap(), "primary_completion");
        assert_eq!(primary.calls(), 1);
        assert_eq!(fallback.calls(), 0);
    }

    #[tokio::test]
    async fn primary_fails_uses_fallback() {
        let primary = Arc::new(MockProvider::failing("primary", || {
            AiError::Unavailable("down".to_string())
        }));
        let fallback = Arc::new(MockProvider::new("fallback"));
        let provider = FallbackProvider::new(
            primary.clone(),
            Some(fallback.clone()),
            Duration::from_secs(5),
        );

        let result = provider.complete(&default_context()).await;
        assert_eq!(result.unwrap(), "fallback_completion");
        assert_eq!(primary.calls(), 1);
        assert_eq!(fallback.calls(), 1);
    }

    #[tokio::test]
    async fn no_fallback_returns_error() {
        let primary = Arc::new(MockProvider::failing("primary", || {
            AiError::Unavailable("down".to_string())
        }));
        let provider = FallbackProvider::new(primary.clone(), None, Duration::from_secs(5));

        let result = provider.complete(&default_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn auth_error_does_not_fallback() {
        let primary = Arc::new(MockProvider::failing("primary", || {
            AiError::AuthenticationFailed("bad key".to_string())
        }));
        let fallback = Arc::new(MockProvider::new("fallback"));
        let provider = FallbackProvider::new(
            primary.clone(),
            Some(fallback.clone()),
            Duration::from_secs(5),
        );

        let result = provider.complete(&default_context()).await;
        assert!(result.is_err());
        // Fallback should NOT have been called for auth errors.
        assert_eq!(fallback.calls(), 0);
    }

    #[tokio::test]
    async fn timeout_triggers_fallback() {
        // Create a provider that hangs forever.
        struct HangingProvider;
        #[async_trait]
        impl AiProvider for HangingProvider {
            async fn complete(&self, _context: &AiContext) -> Result<String, AiError> {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok("should not reach".to_string())
            }
            async fn chat_stream(
                &self,
                _messages: &[Message],
                _context: &AiContext,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<String, AiError>> + Send>>, AiError>
            {
                Err(AiError::Provider("not implemented".to_string()))
            }
            async fn analyze_error(&self, _error: &ErrorContext) -> Result<ErrorAnalysis, AiError> {
                Err(AiError::Provider("not implemented".to_string()))
            }
            async fn is_available(&self) -> bool {
                false
            }
            fn name(&self) -> &str {
                "hanging"
            }
        }

        let fallback = Arc::new(MockProvider::new("fallback"));
        let provider = FallbackProvider::new(
            Arc::new(HangingProvider),
            Some(fallback.clone()),
            Duration::from_millis(50), // very short timeout
        );

        let result = provider.complete(&default_context()).await;
        assert_eq!(result.unwrap(), "fallback_completion");
        assert_eq!(fallback.calls(), 1);
    }
}
