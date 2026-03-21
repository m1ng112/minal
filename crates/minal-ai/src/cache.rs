//! LRU completion cache for instant responses on repeated inputs.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;

use crate::types::AiContext;

/// Default time-to-live for cached completions (5 minutes).
const DEFAULT_TTL: Duration = Duration::from_secs(300);

/// Cache key derived from context to distinguish completions
/// in different working directories or git branches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    input_prefix: String,
    cwd: Option<String>,
    git_branch: Option<String>,
}

impl CacheKey {
    fn from_context(context: &AiContext) -> Self {
        Self {
            input_prefix: context.input_prefix.clone(),
            cwd: context.cwd.clone(),
            git_branch: context
                .git_info
                .as_ref()
                .and_then(|g| g.branch.clone())
                .or_else(|| context.git_branch.clone()),
        }
    }
}

/// A cached completion entry with expiry.
struct CacheEntry {
    completion: String,
    created_at: Instant,
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// LRU cache for AI completion results.
///
/// Keyed by `(input_prefix, cwd, git_branch)` so the same command
/// in different directories produces separate cache entries.
pub struct CompletionCache {
    inner: LruCache<CacheKey, CacheEntry>,
    ttl: Duration,
}

impl CompletionCache {
    /// Creates a new cache with the given capacity.
    ///
    /// A capacity of 0 is treated as 1 (minimum single-entry cache).
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).expect("capacity.max(1) is always >= 1");
        Self {
            inner: LruCache::new(cap),
            ttl: DEFAULT_TTL,
        }
    }

    /// Look up a cached completion for the given context.
    ///
    /// Returns `Some(completion)` on cache hit with a valid TTL,
    /// or `None` on miss / expired entry.
    pub fn get(&mut self, context: &AiContext) -> Option<String> {
        let key = CacheKey::from_context(context);
        if let Some(entry) = self.inner.get(&key) {
            if entry.is_expired() {
                self.inner.pop(&key);
                None
            } else {
                Some(entry.completion.clone())
            }
        } else {
            None
        }
    }

    /// Store a completion result in the cache.
    pub fn put(&mut self, context: &AiContext, completion: String) {
        let key = CacheKey::from_context(context);
        let entry = CacheEntry {
            completion,
            created_at: Instant::now(),
            ttl: self.ttl,
        };
        self.inner.put(key, entry);
    }

    /// Remove all entries whose key prefix starts with the given string.
    pub fn invalidate_prefix(&mut self, prefix: &str) {
        let keys_to_remove: Vec<CacheKey> = self
            .inner
            .iter()
            .filter(|(k, _)| k.input_prefix.starts_with(prefix))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys_to_remove {
            self.inner.pop(&key);
        }
    }

    /// Remove all entries from the cache.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AiContext;
    use std::thread;

    fn make_context(prefix: &str, cwd: Option<&str>, branch: Option<&str>) -> AiContext {
        AiContext {
            input_prefix: prefix.to_string(),
            cwd: cwd.map(String::from),
            git_branch: branch.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn cache_hit_and_miss() {
        let mut cache = CompletionCache::new(16);
        let ctx = make_context("git st", Some("/home"), Some("main"));

        assert!(cache.get(&ctx).is_none());
        cache.put(&ctx, "atus".to_string());
        assert_eq!(cache.get(&ctx).as_deref(), Some("atus"));
    }

    #[test]
    fn different_cwd_is_different_key() {
        let mut cache = CompletionCache::new(16);
        let ctx1 = make_context("ls", Some("/home"), None);
        let ctx2 = make_context("ls", Some("/tmp"), None);

        cache.put(&ctx1, "home_completion".to_string());
        assert!(cache.get(&ctx2).is_none());
        assert_eq!(cache.get(&ctx1).as_deref(), Some("home_completion"));
    }

    #[test]
    fn different_branch_is_different_key() {
        let mut cache = CompletionCache::new(16);
        let ctx1 = make_context("git push", Some("/repo"), Some("main"));
        let ctx2 = make_context("git push", Some("/repo"), Some("feature"));

        cache.put(&ctx1, "origin main".to_string());
        assert!(cache.get(&ctx2).is_none());
    }

    #[test]
    fn lru_eviction() {
        let mut cache = CompletionCache::new(2);
        let ctx1 = make_context("a", None, None);
        let ctx2 = make_context("b", None, None);
        let ctx3 = make_context("c", None, None);

        cache.put(&ctx1, "1".to_string());
        cache.put(&ctx2, "2".to_string());
        cache.put(&ctx3, "3".to_string());

        // ctx1 was evicted (LRU)
        assert!(cache.get(&ctx1).is_none());
        assert_eq!(cache.get(&ctx2).as_deref(), Some("2"));
        assert_eq!(cache.get(&ctx3).as_deref(), Some("3"));
    }

    #[test]
    fn ttl_expiry() {
        let mut cache = CompletionCache::new(16);
        cache.ttl = Duration::from_millis(10);

        let ctx = make_context("test", None, None);
        cache.put(&ctx, "result".to_string());
        assert!(cache.get(&ctx).is_some());

        thread::sleep(Duration::from_millis(20));
        assert!(cache.get(&ctx).is_none());
    }

    #[test]
    fn invalidate_prefix_removes_matching() {
        let mut cache = CompletionCache::new(16);
        let ctx1 = make_context("git status", None, None);
        let ctx2 = make_context("git push", None, None);
        let ctx3 = make_context("ls -la", None, None);

        cache.put(&ctx1, "a".to_string());
        cache.put(&ctx2, "b".to_string());
        cache.put(&ctx3, "c".to_string());

        cache.invalidate_prefix("git");
        assert!(cache.get(&ctx1).is_none());
        assert!(cache.get(&ctx2).is_none());
        assert_eq!(cache.get(&ctx3).as_deref(), Some("c"));
    }

    #[test]
    fn clear_empties_cache() {
        let mut cache = CompletionCache::new(16);
        let ctx = make_context("test", None, None);
        cache.put(&ctx, "val".to_string());
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }
}
