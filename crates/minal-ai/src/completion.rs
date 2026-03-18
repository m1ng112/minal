//! AI completion engine with debounce logic.

use std::time::Instant;

use crate::context::ContextGatherer;

/// Manages AI completion requests with debounce.
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
}

impl CompletionEngine {
    /// Creates a new completion engine with the given debounce time.
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            enabled: true,
            debounce_ms,
            last_input_time: None,
            pending_prefix: None,
            gatherer: ContextGatherer::default(),
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
fn is_bare_prompt(trimmed: &str) -> bool {
    // Common prompt endings: `$`, `%`, `#`, `>`
    // with optional trailing space.
    let stripped = trimmed.trim_end();
    if stripped.is_empty() {
        return true;
    }
    let last_char = stripped.chars().last().unwrap_or(' ');
    matches!(last_char, '$' | '%' | '#' | '>')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let engine = CompletionEngine::new(300);
        assert!(engine.is_enabled());
    }

    #[test]
    fn test_toggle() {
        let mut engine = CompletionEngine::new(300);
        assert!(engine.is_enabled());
        engine.toggle();
        assert!(!engine.is_enabled());
        engine.toggle();
        assert!(engine.is_enabled());
    }

    #[test]
    fn test_empty_input_ignored() {
        let mut engine = CompletionEngine::new(10);
        engine.on_input_changed("");
        assert!(engine.pending_prefix.is_none());
    }

    #[test]
    fn test_prompt_only_ignored() {
        let mut engine = CompletionEngine::new(10);
        engine.on_input_changed("user@host:~$");
        assert!(engine.pending_prefix.is_none());

        engine.on_input_changed("% ");
        assert!(engine.pending_prefix.is_none());
    }

    #[test]
    fn test_valid_input_stored() {
        let mut engine = CompletionEngine::new(10);
        engine.on_input_changed("git sta");
        assert_eq!(engine.pending_prefix.as_deref(), Some("git sta"));
    }

    #[test]
    fn test_debounce_tick() {
        let mut engine = CompletionEngine::new(10);
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
        let mut engine = CompletionEngine::new(300);
        engine.on_input_changed("git sta");
        engine.clear();
        assert!(engine.pending_prefix.is_none());
        assert!(engine.last_input_time.is_none());
    }

    #[test]
    fn test_disabled_ignores_input() {
        let mut engine = CompletionEngine::new(10);
        engine.toggle(); // disable
        engine.on_input_changed("git sta");
        assert!(engine.pending_prefix.is_none());
    }

    #[test]
    fn test_debounce_deadline() {
        let mut engine = CompletionEngine::new(300);
        assert!(engine.debounce_deadline().is_none());

        engine.on_input_changed("git sta");
        let deadline = engine.debounce_deadline();
        assert!(deadline.is_some());
    }
}
