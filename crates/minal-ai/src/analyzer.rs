//! Session error analyzer.
//!
//! Detects errors from terminal command output using pattern matching
//! and exit code analysis. Maintains a rolling buffer of detected errors.

use std::collections::VecDeque;
use std::sync::LazyLock;

use regex::RegexSet;

use crate::types::{CommandRecord, DetectedError, ErrorAnalysis, ErrorCategory};

// -- Static compiled regex patterns (compiled once per process) --

/// Patterns indicating build/compile errors.
static BUILD_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"error\[E\d+\]",          // Rust compiler errors
        r"^error:",                // Generic compile error
        r"SyntaxError:",           // JS/Python syntax error
        r"TypeError:",             // JS/Python type error
        r"CompileError",           // Generic compile error
        r"cannot find symbol",     // Java
        r"undefined reference to", // C/C++ linker
        r"fatal error:",           // C/C++ compiler
        r"Build FAILED",           // .NET
    ])
    .expect("build patterns are valid at compile-time")
});

/// Patterns indicating test failures.
static TEST_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"FAILED",                // Generic test failure
        r"test .+ \.\.\. FAILED", // Rust test
        r"failures:",             // Rust test summary
        r"AssertionError",        // Python/JS assertion
        r"assert!.*failed",       // Rust assert
        r"FAIL ",                 // Jest/Go test
        r"Tests:\s+\d+ failed",   // Jest summary
    ])
    .expect("test patterns are valid at compile-time")
});

/// Patterns indicating permission errors.
static PERMISSION_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"(?i)permission denied",
        r"EACCES",
        r"(?i)operation not permitted",
        r"(?i)access denied",
    ])
    .expect("permission patterns are valid at compile-time")
});

/// Patterns indicating command not found.
static NOT_FOUND_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"command not found",
        r"not found",
        r"No such file or directory",
        r"not recognized as", // Windows
    ])
    .expect("not_found patterns are valid at compile-time")
});

/// Patterns indicating network errors.
static NETWORK_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"(?i)connection refused",
        r"(?i)connection timed out",
        r"(?i)could not resolve host",
        r"ETIMEDOUT",
        r"ECONNREFUSED",
        r"(?i)network is unreachable",
    ])
    .expect("network patterns are valid at compile-time")
});

/// General error keywords for initial detection.
static GENERAL_ERROR_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"(?m)^error:",
        r"(?m)^Error:",
        r"ERROR",
        r"(?m)^fatal:",
        r"FATAL",
        r"(?m)^panic",
        r"Traceback \(most recent call last\)",
        r"goroutine \d+ \[",
        r"at .*:\d+:\d+",
    ])
    .expect("general error patterns are valid at compile-time")
});

/// Analyzes terminal output for errors.
pub struct SessionAnalyzer {
    /// Detected errors (bounded deque, oldest evicted first).
    errors: VecDeque<DetectedError>,
    /// Maximum number of errors to retain.
    max_errors: usize,
}

impl SessionAnalyzer {
    /// Creates a new session analyzer.
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: VecDeque::new(),
            max_errors,
        }
    }

    /// Analyze a completed command for errors.
    ///
    /// Returns `Some(DetectedError)` if an error was detected, `None` otherwise.
    pub fn on_command_completed(&mut self, record: &CommandRecord) -> Option<DetectedError> {
        // Non-zero exit code always triggers detection.
        let has_error_exit = record.exit_code != 0;
        // Check if the output contains error patterns.
        let has_error_pattern = GENERAL_ERROR_PATTERNS.is_match(&record.output);

        if !has_error_exit && !has_error_pattern {
            return None;
        }

        let category = classify(record);
        let summary = extract_summary(record);
        let output_snippet = truncate_str(&record.output, 500);

        let error = DetectedError {
            category,
            command: record.command.clone(),
            exit_code: record.exit_code,
            summary,
            output_snippet,
            timestamp: record.timestamp,
            ai_analysis: None,
        };

        // Evict oldest if at capacity.
        if self.errors.len() >= self.max_errors {
            self.errors.pop_front();
        }
        self.errors.push_back(error.clone());

        Some(error)
    }

    /// Returns the number of detected errors.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Returns an iterator over all detected errors.
    pub fn errors(&self) -> impl Iterator<Item = &DetectedError> {
        self.errors.iter()
    }

    /// Clears all detected errors.
    pub fn clear(&mut self) {
        self.errors.clear();
    }

    /// Update the AI analysis for the most recent error.
    pub fn update_latest_analysis(&mut self, analysis: ErrorAnalysis) {
        if let Some(last) = self.errors.back_mut() {
            last.ai_analysis = Some(analysis);
        }
    }

    /// Dismiss (remove) an error by index.
    pub fn dismiss(&mut self, index: usize) {
        if index < self.errors.len() {
            self.errors.remove(index);
        }
    }
}

/// Classify an error based on output patterns.
///
/// Priority: Permission > NotFound > Network > Test > Build > Runtime.
/// Permission and NotFound are checked first because they provide the most
/// actionable classification even when other patterns also match.
fn classify(record: &CommandRecord) -> ErrorCategory {
    let output = &record.output;

    if PERMISSION_PATTERNS.is_match(output) {
        return ErrorCategory::Permission;
    }
    if NOT_FOUND_PATTERNS.is_match(output) {
        return ErrorCategory::NotFound;
    }
    if NETWORK_PATTERNS.is_match(output) {
        return ErrorCategory::Network;
    }
    if TEST_PATTERNS.is_match(output) {
        return ErrorCategory::Test;
    }
    if BUILD_PATTERNS.is_match(output) {
        return ErrorCategory::Build;
    }

    // Non-zero exit code with general error patterns -> Runtime.
    if record.exit_code != 0 {
        return ErrorCategory::Runtime;
    }

    ErrorCategory::Unknown
}

/// Extract a brief one-line summary from the command output.
fn extract_summary(record: &CommandRecord) -> String {
    for line in record.output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if GENERAL_ERROR_PATTERNS.is_match(trimmed)
            || BUILD_PATTERNS.is_match(trimmed)
            || TEST_PATTERNS.is_match(trimmed)
            || PERMISSION_PATTERNS.is_match(trimmed)
            || NOT_FOUND_PATTERNS.is_match(trimmed)
            || NETWORK_PATTERNS.is_match(trimmed)
        {
            return truncate_str(trimmed, 120);
        }
    }

    // Fallback: use exit code description.
    if record.exit_code != 0 {
        return format!("Command exited with code {}", record.exit_code);
    }

    "Error detected".to_string()
}

/// Truncate a string to the given max number of Unicode scalar values.
///
/// Uses `char_indices` to avoid panicking on multi-byte UTF-8 boundaries.
fn truncate_str(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => format!("{}...", &s[..byte_idx]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(command: &str, output: &str, exit_code: i32) -> CommandRecord {
        CommandRecord {
            command: command.to_string(),
            output: output.to_string(),
            exit_code,
            timestamp: 1000,
            cwd: Some("/tmp".to_string()),
        }
    }

    #[test]
    fn non_zero_exit_code_detected() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("false", "", 1);
        let result = analyzer.on_command_completed(&record);
        assert!(result.is_some());
        assert_eq!(result.unwrap().exit_code, 1);
    }

    #[test]
    fn zero_exit_no_error_patterns_not_detected() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("echo hello", "hello\n", 0);
        let result = analyzer.on_command_completed(&record);
        assert!(result.is_none());
    }

    #[test]
    fn zero_exit_with_error_keyword_detected() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("some-tool", "error: something went wrong\n", 0);
        let result = analyzer.on_command_completed(&record);
        assert!(result.is_some());
    }

    #[test]
    fn classify_rust_build_error() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record(
            "cargo build",
            "error[E0308]: mismatched types\n  --> src/main.rs:10:5\n",
            101,
        );
        let result = analyzer.on_command_completed(&record).unwrap();
        assert_eq!(result.category, ErrorCategory::Build);
    }

    #[test]
    fn classify_test_failure() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("cargo test", "test my_test ... FAILED\nfailures:\n", 101);
        let result = analyzer.on_command_completed(&record).unwrap();
        assert_eq!(result.category, ErrorCategory::Test);
    }

    #[test]
    fn classify_permission_denied() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record(
            "cat /root/secret",
            "cat: /root/secret: Permission denied\n",
            1,
        );
        let result = analyzer.on_command_completed(&record).unwrap();
        assert_eq!(result.category, ErrorCategory::Permission);
    }

    #[test]
    fn classify_command_not_found() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("nonexistent", "zsh: command not found: nonexistent\n", 127);
        let result = analyzer.on_command_completed(&record).unwrap();
        assert_eq!(result.category, ErrorCategory::NotFound);
    }

    #[test]
    fn classify_network_error() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record(
            "curl http://localhost:9999",
            "curl: (7) Failed to connect: Connection refused\n",
            7,
        );
        let result = analyzer.on_command_completed(&record).unwrap();
        assert_eq!(result.category, ErrorCategory::Network);
    }

    #[test]
    fn classify_runtime_error() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("./myapp", "Segmentation fault\n", 139);
        let result = analyzer.on_command_completed(&record).unwrap();
        assert_eq!(result.category, ErrorCategory::Runtime);
    }

    #[test]
    fn classify_python_traceback() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record(
            "python script.py",
            "Traceback (most recent call last):\n  File \"script.py\", line 1\nNameError: name 'x' is not defined\n",
            1,
        );
        let result = analyzer.on_command_completed(&record).unwrap();
        // Python traceback is a runtime error (general pattern match).
        assert!(
            result.category == ErrorCategory::Runtime || result.category == ErrorCategory::Build
        );
    }

    #[test]
    fn ring_buffer_eviction() {
        let mut analyzer = SessionAnalyzer::new(3);
        for i in 0..5 {
            let record = make_record(&format!("cmd{i}"), "error: fail\n", 1);
            analyzer.on_command_completed(&record);
        }
        assert_eq!(analyzer.error_count(), 3);
        // Oldest errors should have been evicted.
        let cmds: Vec<String> = analyzer.errors().map(|e| e.command.clone()).collect();
        assert_eq!(cmds, vec!["cmd2", "cmd3", "cmd4"]);
    }

    #[test]
    fn clear_removes_all() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("false", "", 1);
        analyzer.on_command_completed(&record);
        assert_eq!(analyzer.error_count(), 1);
        analyzer.clear();
        assert_eq!(analyzer.error_count(), 0);
    }

    #[test]
    fn summary_extraction_from_rust_error() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record(
            "cargo build",
            "   Compiling minal v0.1.0\nerror[E0308]: mismatched types\n  --> src/main.rs:10:5\n",
            101,
        );
        let result = analyzer.on_command_completed(&record).unwrap();
        assert!(result.summary.contains("error[E0308]"));
    }

    #[test]
    fn summary_fallback_for_empty_output() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("false", "", 1);
        let result = analyzer.on_command_completed(&record).unwrap();
        assert!(result.summary.contains("exited with code 1"));
    }

    #[test]
    fn update_latest_analysis() {
        let mut analyzer = SessionAnalyzer::new(50);
        let record = make_record("false", "error: bad\n", 1);
        analyzer.on_command_completed(&record);

        let analysis = ErrorAnalysis {
            explanation: "The command failed".to_string(),
            suggestions: vec!["Try again".to_string()],
            confidence: 0.9,
        };
        analyzer.update_latest_analysis(analysis);

        let last = analyzer.errors().last().unwrap();
        assert!(last.ai_analysis.is_some());
        assert_eq!(
            last.ai_analysis.as_ref().unwrap().explanation,
            "The command failed"
        );
    }

    #[test]
    fn dismiss_error() {
        let mut analyzer = SessionAnalyzer::new(50);
        for i in 0..3 {
            let record = make_record(&format!("cmd{i}"), "error: fail\n", 1);
            analyzer.on_command_completed(&record);
        }
        assert_eq!(analyzer.error_count(), 3);
        analyzer.dismiss(1);
        assert_eq!(analyzer.error_count(), 2);
        let cmds: Vec<String> = analyzer.errors().map(|e| e.command.clone()).collect();
        assert_eq!(cmds, vec!["cmd0", "cmd2"]);
    }

    #[test]
    fn error_category_display() {
        assert_eq!(ErrorCategory::Build.to_string(), "Build");
        assert_eq!(ErrorCategory::NotFound.to_string(), "Not Found");
        assert_eq!(ErrorCategory::Unknown.to_string(), "Unknown");
    }
}
