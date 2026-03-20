//! Terminal context gathering for AI completion.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use minal_config::AiPrivacyConfig;
use minal_core::term::Terminal;

use crate::types::{AiContext, CommandRecord, GitInfo, ProjectType};

/// Collects context from the terminal and environment for AI requests.
///
/// Gathers CWD, git info, project type, shell/OS, command history,
/// and environment hints -- all filtered through privacy settings.
pub struct ContextCollector {
    /// Number of recent output lines to include.
    pub max_context_lines: usize,
    /// Privacy settings controlling what context is collected.
    privacy: AiPrivacyConfig,
    /// Ring buffer of recent commands.
    command_history: VecDeque<CommandRecord>,
    /// Current working directory (updated externally via `set_cwd`).
    cached_cwd: Option<String>,
    /// Detected shell (cached at initialization).
    cached_shell: Option<String>,
    /// Detected OS (cached at initialization).
    cached_os: Option<String>,
}

/// Backward-compatible alias for [`ContextCollector`].
pub type ContextGatherer = ContextCollector;

impl Default for ContextCollector {
    fn default() -> Self {
        Self::new(AiPrivacyConfig::default())
    }
}

impl ContextCollector {
    /// Creates a new context collector with the given privacy settings.
    pub fn new(privacy: AiPrivacyConfig) -> Self {
        Self {
            max_context_lines: 20,
            privacy,
            command_history: VecDeque::new(),
            cached_cwd: None,
            cached_shell: detect_shell(),
            cached_os: detect_os(),
        }
    }

    /// Sets the current working directory (typically from OSC 7 or initial PTY CWD).
    pub fn set_cwd(&mut self, cwd: String) {
        self.cached_cwd = Some(cwd);
    }

    /// Returns the current working directory, if known.
    pub fn cwd(&self) -> Option<&str> {
        self.cached_cwd.as_deref()
    }

    /// Records a completed command in the history ring buffer.
    ///
    /// Output is truncated to `privacy.max_output_chars`. Old entries
    /// are evicted when the buffer exceeds `privacy.max_command_history`.
    pub fn record_command(&mut self, mut record: CommandRecord) {
        // Truncate output to privacy limit.
        if record.output.len() > self.privacy.max_output_chars {
            record.output.truncate(self.privacy.max_output_chars);
            record.output.push_str("...(truncated)");
        }

        self.command_history.push_back(record);
        while self.command_history.len() > self.privacy.max_command_history {
            self.command_history.pop_front();
        }
    }

    /// Gather completion context from the terminal state.
    ///
    /// Applies privacy filtering to all collected context.
    pub fn gather(&self, terminal: &Terminal) -> AiContext {
        let input_prefix = terminal.cursor_line_prefix();

        // Read recent output lines from the grid.
        let recent_output = self.collect_terminal_output(terminal);

        // Conditionally collect CWD.
        let cwd = if self.privacy.send_cwd {
            self.cached_cwd.clone()
        } else {
            None
        };

        // Conditionally collect git info.
        let (git_info, git_branch) = if self.privacy.send_git_status {
            if let Some(ref cwd_path) = self.cached_cwd {
                let info = collect_git_info(cwd_path);
                let branch = info.as_ref().and_then(|i| i.branch.clone());
                (info, branch)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Detect project type from CWD.
        let project_type = self.cached_cwd.as_deref().and_then(detect_project_type);

        // Conditionally collect env hints.
        let env_hints = if self.privacy.send_env {
            collect_env_hints(&self.privacy.exclude_patterns)
        } else {
            Vec::new()
        };

        AiContext {
            cwd,
            input_prefix,
            recent_output,
            shell: self.cached_shell.clone(),
            os: self.cached_os.clone(),
            git_branch,
            git_info,
            project_type,
            command_history: self.command_history.iter().cloned().collect(),
            env_hints,
        }
    }

    /// Collect terminal output lines, applying privacy filters.
    fn collect_terminal_output(&self, terminal: &Terminal) -> Vec<String> {
        let grid = terminal.grid();
        let cursor_row = terminal.cursor().row;
        let mut lines = Vec::new();
        let mut total_chars = 0usize;

        let start_row = cursor_row.saturating_sub(self.max_context_lines);
        for row_idx in start_row..cursor_row {
            if let Some(row) = grid.row(row_idx) {
                let mut line = String::new();
                for col in 0..grid.cols() {
                    if let Some(cell) = row.get(col) {
                        line.push(cell.c);
                    }
                }
                let trimmed = line.trim_end().to_string();
                if trimmed.is_empty() {
                    continue;
                }

                // Apply exclude patterns.
                if self.matches_exclude_pattern(&trimmed) {
                    continue;
                }

                // Enforce max_output_chars budget.
                if total_chars + trimmed.len() > self.privacy.max_output_chars {
                    break;
                }
                total_chars += trimmed.len();
                lines.push(trimmed);
            }
        }

        lines
    }

    /// Check if a line matches any exclude pattern.
    fn matches_exclude_pattern(&self, line: &str) -> bool {
        for pattern in &self.privacy.exclude_patterns {
            if pattern_matches(pattern, line) {
                return true;
            }
        }
        false
    }
}

/// Simple glob-like pattern matching.
///
/// Supports `*` as a wildcard for any sequence of characters.
/// Matches if the pattern appears anywhere in the text as a substring pattern.
fn pattern_matches(pattern: &str, text: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        // "*.env" matches anything ending with ".env"
        text.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        // "credentials*" matches anything starting with "credentials"
        text.starts_with(prefix)
    } else {
        // Exact substring match.
        text.contains(pattern)
    }
}

/// Detect the user's shell from the `$SHELL` environment variable.
fn detect_shell() -> Option<String> {
    std::env::var("SHELL").ok().map(|s| {
        Path::new(&s)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&s)
            .to_string()
    })
}

/// Detect the operating system using compile-time information.
///
/// Avoids spawning subprocesses at startup; the OS name is sufficient
/// for AI context without the exact version.
fn detect_os() -> Option<String> {
    if cfg!(target_os = "macos") {
        Some("macOS".to_string())
    } else if cfg!(target_os = "linux") {
        Some("Linux".to_string())
    } else if cfg!(target_os = "windows") {
        Some("Windows".to_string())
    } else {
        Some(std::env::consts::OS.to_string())
    }
}

/// Timeout for git commands.
const GIT_TIMEOUT: Duration = Duration::from_millis(50);

/// Collect git repository information from the given directory.
fn collect_git_info(cwd: &str) -> Option<GitInfo> {
    let branch = collect_git_branch(cwd);
    let status_summary = collect_git_status_summary(cwd);

    if branch.is_none() && status_summary.is_none() {
        return None;
    }

    Some(GitInfo {
        branch,
        status_summary,
    })
}

/// Run a git command with a timeout, returning stdout on success.
fn run_git_with_timeout(cwd: &str, args: &[&str]) -> Option<Vec<u8>> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Read stdout before waiting (avoid double-wait).
    let mut stdout_buf = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        let _ = stdout.read_to_end(&mut stdout_buf);
    }

    match child.wait_timeout(GIT_TIMEOUT) {
        Ok(Some(status)) if status.success() => Some(stdout_buf),
        Ok(Some(_)) => None,
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            tracing::debug!("git command timed out: {:?}", args);
            None
        }
        Err(e) => {
            tracing::debug!("git command wait failed: {e}");
            None
        }
    }
}

/// Get the current git branch.
fn collect_git_branch(cwd: &str) -> Option<String> {
    let stdout = run_git_with_timeout(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = String::from_utf8_lossy(&stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

/// Get a summary of git working tree status.
fn collect_git_status_summary(cwd: &str) -> Option<String> {
    let stdout = run_git_with_timeout(cwd, &["status", "--porcelain=v1"])?;
    let text = String::from_utf8_lossy(&stdout);

    let mut modified = 0u32;
    let mut staged = 0u32;
    let mut untracked = 0u32;

    for line in text.lines() {
        if line.len() < 2 {
            continue;
        }
        let index = line.as_bytes()[0];
        let worktree = line.as_bytes()[1];

        if line.starts_with("??") {
            untracked += 1;
        } else {
            if index != b' ' && index != b'?' {
                staged += 1;
            }
            if worktree != b' ' && worktree != b'?' {
                modified += 1;
            }
        }
    }

    if modified == 0 && staged == 0 && untracked == 0 {
        return Some("clean".to_string());
    }

    let mut parts = Vec::new();
    if staged > 0 {
        parts.push(format!("{staged} staged"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    if untracked > 0 {
        parts.push(format!("{untracked} untracked"));
    }
    Some(parts.join(", "))
}

/// Detect the project type from marker files in the given directory.
fn detect_project_type(cwd: &str) -> Option<ProjectType> {
    let path = Path::new(cwd);

    if path.join("Cargo.toml").exists() {
        Some(ProjectType::Rust)
    } else if path.join("package.json").exists() {
        Some(ProjectType::Node)
    } else if path.join("pyproject.toml").exists()
        || path.join("setup.py").exists()
        || path.join("requirements.txt").exists()
    {
        Some(ProjectType::Python)
    } else if path.join("go.mod").exists() {
        Some(ProjectType::Go)
    } else if path.join("pom.xml").exists() || path.join("build.gradle").exists() {
        Some(ProjectType::Java)
    } else if path.join("Gemfile").exists() {
        Some(ProjectType::Ruby)
    } else {
        None
    }
}

/// Environment variables relevant to development context.
const DEV_ENV_VARS: &[&str] = &[
    "VIRTUAL_ENV",
    "CONDA_DEFAULT_ENV",
    "NODE_ENV",
    "RUSTUP_TOOLCHAIN",
    "GOPATH",
    "JAVA_HOME",
    "EDITOR",
    "TERM",
];

/// Collect development-relevant environment variable hints.
fn collect_env_hints(exclude_patterns: &[String]) -> Vec<(String, String)> {
    let mut hints = Vec::new();
    for &var in DEV_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            // Check exclude patterns against the variable name.
            let excluded = exclude_patterns.iter().any(|p| pattern_matches(p, var));
            if !excluded {
                hints.push((var.to_string(), val));
            }
        }
    }
    hints
}

/// Extension trait for `std::process::Child` to add timeout support.
trait ChildExt {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl ChildExt for std::process::Child {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait()? {
                Some(status) => return Ok(Some(status)),
                None => {
                    if start.elapsed() >= timeout {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gather_empty_terminal() {
        let terminal = Terminal::new(24, 80);
        let collector = ContextCollector::default();
        let ctx = collector.gather(&terminal);
        assert!(ctx.input_prefix.is_empty());
        assert!(ctx.recent_output.is_empty());
        assert!(ctx.cwd.is_none());
    }

    #[test]
    fn gather_with_input() {
        let mut terminal = Terminal::new(24, 80);
        for c in "git sta".chars() {
            terminal.input_char(c);
        }
        let collector = ContextCollector::default();
        let ctx = collector.gather(&terminal);
        assert_eq!(ctx.input_prefix, "git sta");
    }

    #[test]
    fn gather_with_recent_output() {
        let mut terminal = Terminal::new(24, 80);
        for c in "file.txt".chars() {
            terminal.input_char(c);
        }
        terminal.linefeed();
        terminal.carriage_return();

        let collector = ContextCollector::default();
        let ctx = collector.gather(&terminal);
        assert_eq!(ctx.recent_output.len(), 1);
        assert_eq!(ctx.recent_output[0], "file.txt");
    }

    #[test]
    fn gather_with_cwd_set() {
        let terminal = Terminal::new(24, 80);
        let mut collector = ContextCollector::default();
        collector.set_cwd("/home/user/project".to_string());
        let ctx = collector.gather(&terminal);
        assert_eq!(ctx.cwd.as_deref(), Some("/home/user/project"));
    }

    #[test]
    fn gather_with_privacy_cwd_disabled() {
        let terminal = Terminal::new(24, 80);
        let privacy = AiPrivacyConfig {
            send_cwd: false,
            ..AiPrivacyConfig::default()
        };
        let mut collector = ContextCollector::new(privacy);
        collector.set_cwd("/home/user/project".to_string());
        let ctx = collector.gather(&terminal);
        assert!(ctx.cwd.is_none());
    }

    #[test]
    fn gather_with_privacy_git_disabled() {
        let terminal = Terminal::new(24, 80);
        let privacy = AiPrivacyConfig {
            send_git_status: false,
            ..AiPrivacyConfig::default()
        };
        let mut collector = ContextCollector::new(privacy);
        collector.set_cwd("/tmp".to_string());
        let ctx = collector.gather(&terminal);
        assert!(ctx.git_info.is_none());
        assert!(ctx.git_branch.is_none());
    }

    #[test]
    fn gather_with_privacy_env_disabled() {
        let terminal = Terminal::new(24, 80);
        let privacy = AiPrivacyConfig {
            send_env: false,
            ..AiPrivacyConfig::default()
        };
        let collector = ContextCollector::new(privacy);
        let ctx = collector.gather(&terminal);
        assert!(ctx.env_hints.is_empty());
    }

    #[test]
    fn record_command_ring_buffer() {
        let privacy = AiPrivacyConfig {
            max_command_history: 3,
            ..AiPrivacyConfig::default()
        };
        let mut collector = ContextCollector::new(privacy);

        for i in 0..5 {
            collector.record_command(CommandRecord {
                command: format!("cmd{i}"),
                output: String::new(),
                exit_code: 0,
                timestamp: i as u64,
                cwd: None,
            });
        }

        let terminal = Terminal::new(24, 80);
        let ctx = collector.gather(&terminal);
        assert_eq!(ctx.command_history.len(), 3);
        assert_eq!(ctx.command_history[0].command, "cmd2");
        assert_eq!(ctx.command_history[2].command, "cmd4");
    }

    #[test]
    fn record_command_output_truncation() {
        let privacy = AiPrivacyConfig {
            max_output_chars: 10,
            ..AiPrivacyConfig::default()
        };
        let mut collector = ContextCollector::new(privacy);

        collector.record_command(CommandRecord {
            command: "echo".to_string(),
            output: "a".repeat(20),
            exit_code: 0,
            timestamp: 0,
            cwd: None,
        });

        let terminal = Terminal::new(24, 80);
        let ctx = collector.gather(&terminal);
        assert!(ctx.command_history[0].output.starts_with("aaaaaaaaaa"));
        assert!(ctx.command_history[0].output.contains("(truncated)"));
    }

    #[test]
    fn detect_project_type_rust() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("Cargo.toml"), "").expect("write file");
        let result = detect_project_type(dir.path().to_str().expect("path"));
        assert_eq!(result, Some(ProjectType::Rust));
    }

    #[test]
    fn detect_project_type_node() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(dir.path().join("package.json"), "{}").expect("write file");
        let result = detect_project_type(dir.path().to_str().expect("path"));
        assert_eq!(result, Some(ProjectType::Node));
    }

    #[test]
    fn detect_project_type_none() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let result = detect_project_type(dir.path().to_str().expect("path"));
        assert_eq!(result, None);
    }

    #[test]
    fn pattern_matches_suffix_wildcard() {
        assert!(pattern_matches("*.env", "test.env"));
        assert!(pattern_matches("*.env", ".env"));
        assert!(!pattern_matches("*.env", "env.test"));
    }

    #[test]
    fn pattern_matches_prefix_wildcard() {
        assert!(pattern_matches("credentials*", "credentials.json"));
        assert!(!pattern_matches("credentials*", "my credentials file"));
        assert!(!pattern_matches("credentials*", "test"));
    }

    #[test]
    fn detect_shell_returns_some() {
        // $SHELL is typically set in CI and development environments.
        let shell = detect_shell();
        // May be None in some CI environments, so just check it doesn't panic.
        if let Some(s) = shell {
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn detect_os_returns_some() {
        let os = detect_os();
        assert!(os.is_some());
    }

    #[test]
    fn shell_and_os_cached_on_init() {
        let collector = ContextCollector::default();
        // Shell and OS are cached at initialization.
        let terminal = Terminal::new(24, 80);
        let ctx = collector.gather(&terminal);
        assert!(ctx.os.is_some());
        // shell might be None in some CI environments
    }

    #[test]
    fn gather_performance_baseline() {
        let terminal = Terminal::new(24, 80);
        let mut collector = ContextCollector::default();
        collector.set_cwd("/tmp".to_string());

        let start = std::time::Instant::now();
        let _ctx = collector.gather(&terminal);
        let elapsed = start.elapsed();

        // Must complete within 100ms.
        assert!(
            elapsed.as_millis() < 100,
            "Context collection took {elapsed:?}, exceeding 100ms budget"
        );
    }

    #[test]
    fn exclude_patterns_filter_output() {
        let mut terminal = Terminal::new(24, 80);
        // Write a line containing ".env"
        for c in "SECRET=abc.env".chars() {
            terminal.input_char(c);
        }
        terminal.linefeed();
        terminal.carriage_return();
        // Write a normal line
        for c in "hello world".chars() {
            terminal.input_char(c);
        }
        terminal.linefeed();
        terminal.carriage_return();

        let privacy = AiPrivacyConfig {
            exclude_patterns: vec!["*.env".to_string()],
            ..AiPrivacyConfig::default()
        };
        let collector = ContextCollector::new(privacy);
        let ctx = collector.gather(&terminal);

        // The .env line should be filtered out.
        assert_eq!(ctx.recent_output.len(), 1);
        assert_eq!(ctx.recent_output[0], "hello world");
    }

    #[test]
    fn format_completion_prompt_includes_new_fields() {
        let ctx = AiContext {
            cwd: Some("/home/user/project".to_string()),
            input_prefix: "cargo b".to_string(),
            recent_output: vec!["$ ls".to_string()],
            shell: Some("zsh".to_string()),
            os: Some("macOS 14.0".to_string()),
            git_branch: None,
            git_info: Some(GitInfo {
                branch: Some("main".to_string()),
                status_summary: Some("2 modified".to_string()),
            }),
            project_type: Some(ProjectType::Rust),
            command_history: vec![CommandRecord {
                command: "cargo test".to_string(),
                output: String::new(),
                exit_code: 0,
                timestamp: 0,
                cwd: None,
            }],
            env_hints: vec![],
        };

        let prompt = ctx.format_completion_prompt();
        assert!(prompt.contains("Git branch: main"));
        assert!(prompt.contains("Git status: 2 modified"));
        assert!(prompt.contains("Project type: Rust"));
        assert!(prompt.contains("cargo test"));
        assert!(prompt.contains("cargo b"));
    }
}
