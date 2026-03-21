//! AI completion engine with debounce logic and LRU cache.

use std::time::Instant;

use crate::cache::CompletionCache;
use crate::context::ContextGatherer;
use crate::types::AiContext;

/// Manages AI completion requests with debounce and caching.
pub struct CompletionEngine {
    /// Whether AI completion is enabled.
    enabled: bool,
    /// Debounce time in milliseconds.
    debounce_ms: u64,
    /// Timestamp of the last input change.
    last_input_time: Option<Instant>,
    /// The pending input prefix awaiting debounce.
    pending_prefix: Option<String>,
    /// Context gatherer for reading terminal state.
    pub gatherer: ContextGatherer,
    /// LRU cache for completion results.
    cache: CompletionCache,
    /// Whether a prompt has been detected via OSC 133;A.
    prompt_detected: bool,
    /// Pre-gathered context from the last prompt detection.
    prefetched_context: Option<AiContext>,
}

impl CompletionEngine {
    /// Creates a new completion engine with the given debounce time and cache capacity.
    pub fn new(debounce_ms: u64, cache_size: usize) -> Self {
        Self {
            enabled: true,
            debounce_ms,
            last_input_time: None,
            pending_prefix: None,
            gatherer: ContextGatherer::default(),
            cache: CompletionCache::new(cache_size),
            prompt_detected: false,
            prefetched_context: None,
        }
    }

    /// Called when the user's input line changes.
    ///
    /// Records the prefix and resets the debounce timer. Ignores empty input
    /// or lines that look like bare prompts (ending with `$`, `%`, `#`, or `>`
    /// followed by optional space with no further content).
    pub fn on_input_changed(&mut self, prefix: &str) {
        if !self.enabled {
            return;
        }

        let trimmed = prefix.trim();
        if trimmed.is_empty() {
            self.clear();
            return;
        }

        // Check if this looks like just a prompt with nothing after it.
        if is_bare_prompt(trimmed) {
            self.clear();
            return;
        }

        self.pending_prefix = Some(prefix.to_string());
        self.last_input_time = Some(Instant::now());
    }

    /// Check if the debounce period has elapsed and return the pending prefix.
    ///
    /// Returns `Some(prefix)` if a completion should be triggered, clearing
    /// the pending state. Returns `None` otherwise.
    pub fn tick(&mut self) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let last_time = self.last_input_time?;
        let elapsed = last_time.elapsed().as_millis() as u64;

        if elapsed >= self.debounce_ms {
            self.last_input_time = None;
            self.pending_prefix.take()
        } else {
            None
        }
    }

    /// Toggle AI completion on/off.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.clear();
        }
    }

    /// Whether AI completion is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Check the cache for a completion matching the given context.
    ///
    /// Returns `Some(completion)` on cache hit, `None` on miss.
    pub fn check_cache(&mut self, context: &AiContext) -> Option<String> {
        self.cache.get(context)
    }

    /// Store a completion result in the cache.
    pub fn cache_completion(&mut self, context: &AiContext, completion: String) {
        self.cache.put(context, completion);
    }

    /// Called when OSC 133;A (prompt start) is detected.
    pub fn on_prompt_detected(&mut self) {
        self.prompt_detected = true;
    }

    /// Called when a command starts executing (OSC 133;C).
    pub fn on_command_execute(&mut self) {
        self.prompt_detected = false;
        self.prefetched_context = None;
    }

    /// Whether a prompt is currently active (OSC 133;A was received).
    pub fn is_prompt_active(&self) -> bool {
        self.prompt_detected
    }

    /// Store pre-gathered context for reuse in the next completion request.
    pub fn set_prefetched_context(&mut self, context: AiContext) {
        self.prefetched_context = Some(context);
    }

    /// Take the prefetched context, if any.
    pub fn take_prefetched_context(&mut self) -> Option<AiContext> {
        self.prefetched_context.take()
    }

    /// Clear all pending state.
    pub fn clear(&mut self) {
        self.pending_prefix = None;
        self.last_input_time = None;
    }

    /// Check whether a debounce deadline is pending and return the instant
    /// at which it will expire, so the caller can set `WaitUntil`.
    pub fn debounce_deadline(&self) -> Option<Instant> {
        let last_time = self.last_input_time?;
        if self.pending_prefix.is_some() {
            Some(last_time + std::time::Duration::from_millis(self.debounce_ms))
        } else {
            None
        }
    }
}

/// Returns `true` if the trimmed text looks like a bare shell prompt
/// with no user-typed content after it.
///
/// Only matches when a prompt character (`$`, `%`, `#`, `>`) appears
/// at the end with no space-separated content following it. This avoids
/// false positives on commands like `echo $HOME` or `git log --format=%H`.
fn is_bare_prompt(trimmed: &str) -> bool {
    let stripped = trimmed.trim_end();
    if stripped.is_empty() {
        return true;
    }

    // Check if the line ends with a prompt character, and that
    // there is no space followed by content after the last prompt char.
    // e.g. "user@host:~$" is a bare prompt, but "$ ls" is not.
    for suffix in &["$ ", "% ", "# ", "> "] {
        if let Some(pos) = stripped.rfind(suffix) {
            // There's content after the prompt — not bare.
            let after = &stripped[pos + suffix.len()..];
            if !after.trim().is_empty() {
                return false;
            }
        }
    }

    // Check if last char is a prompt char with nothing after.
    let last_char = stripped.as_bytes()[stripped.len() - 1];
    matches!(last_char, b'$' | b'%' | b'#' | b'>')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let engine = CompletionEngine::new(300, 256);
        assert!(engine.is_enabled());
    }

    #[test]
    fn test_toggle() {
        let mut engine = CompletionEngine::new(300, 256);
        assert!(engine.is_enabled());
        engine.toggle();
        assert!(!engine.is_enabled());
        engine.toggle();
        assert!(engine.is_enabled());
    }

    #[test]
    fn test_empty_input_ignored() {
        let mut engine = CompletionEngine::new(10, 256);
        engine.on_input_changed("");
        assert!(engine.pending_prefix.is_none());
    }

    #[test]
    fn test_prompt_only_ignored() {
        let mut engine = CompletionEngine::new(10, 256);
        engine.on_input_changed("user@host:~$");
        assert!(engine.pending_prefix.is_none());

        engine.on_input_changed("% ");
        assert!(engine.pending_prefix.is_none());
    }

    #[test]
    fn test_valid_input_stored() {
        let mut engine = CompletionEngine::new(10, 256);
        engine.on_input_changed("git sta");
        assert_eq!(engine.pending_prefix.as_deref(), Some("git sta"));
    }

    #[test]
    fn test_commands_with_special_chars_not_bare_prompt() {
        // These should NOT be detected as bare prompts.
        assert!(!is_bare_prompt("$ git status"));
        assert!(!is_bare_prompt("echo $HOME"));
        assert!(!is_bare_prompt("git log --format=%H"));
    }

    #[test]
    fn test_debounce_tick() {
        let mut engine = CompletionEngine::new(10, 256);
        engine.on_input_changed("git sta");

        // Immediately after, debounce hasn't elapsed.
        // (This is timing-dependent but 10ms is generous.)
        // Wait for debounce to elapse.
        thread::sleep(Duration::from_millis(20));

        let result = engine.tick();
        assert_eq!(result.as_deref(), Some("git sta"));

        // After tick, state should be cleared.
        assert!(engine.tick().is_none());
    }

    #[test]
    fn test_clear() {
        let mut engine = CompletionEngine::new(300, 256);
        engine.on_input_changed("git sta");
        engine.clear();
        assert!(engine.pending_prefix.is_none());
        assert!(engine.last_input_time.is_none());
    }

    #[test]
    fn test_disabled_ignores_input() {
        let mut engine = CompletionEngine::new(10, 256);
        engine.toggle(); // disable
        engine.on_input_changed("git sta");
        assert!(engine.pending_prefix.is_none());
    }

    #[test]
    fn test_debounce_deadline() {
        let mut engine = CompletionEngine::new(300, 256);
        assert!(engine.debounce_deadline().is_none());

        engine.on_input_changed("git sta");
        let deadline = engine.debounce_deadline();
        assert!(deadline.is_some());
    }

    #[test]
    fn test_cache_integration() {
        let mut engine = CompletionEngine::new(300, 256);
        let ctx = AiContext {
            input_prefix: "git sta".to_string(),
            cwd: Some("/home".to_string()),
            ..Default::default()
        };
        assert!(engine.check_cache(&ctx).is_none());

        engine.cache_completion(&ctx, "tus".to_string());
        assert_eq!(engine.check_cache(&ctx).as_deref(), Some("tus"));
    }

    #[test]
    fn test_prompt_detection_lifecycle() {
        let mut engine = CompletionEngine::new(300, 256);
        assert!(!engine.is_prompt_active());

        engine.on_prompt_detected();
        assert!(engine.is_prompt_active());

        engine.on_command_execute();
        assert!(!engine.is_prompt_active());
    }

    #[test]
    fn test_prefetched_context() {
        let mut engine = CompletionEngine::new(300, 256);
        assert!(engine.take_prefetched_context().is_none());

        let ctx = AiContext {
            cwd: Some("/project".to_string()),
            ..Default::default()
        };
        engine.set_prefetched_context(ctx);
        let taken = engine.take_prefetched_context();
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().cwd.as_deref(), Some("/project"));

        // Second take returns None
        assert!(engine.take_prefetched_context().is_none());
    }
}
