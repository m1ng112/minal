//! Shared types for AI providers.

use serde::{Deserialize, Serialize};

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message sender.
    pub role: Role,
    /// The message content.
    pub content: String,
}

/// Role of a message sender.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions to the model).
    System,
    /// User message.
    User,
    /// Assistant (model) response.
    Assistant,
}

/// A record of a command executed in the terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    /// The command that was executed.
    pub command: String,
    /// Truncated output of the command.
    pub output: String,
    /// Exit code of the command.
    pub exit_code: i32,
    /// Unix timestamp (seconds) when the command was executed.
    pub timestamp: u64,
    /// Working directory when the command was run.
    pub cwd: Option<String>,
}

/// Detected project type based on marker files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectType {
    /// Rust project (`Cargo.toml`).
    Rust,
    /// Node.js project (`package.json`).
    Node,
    /// Python project (`pyproject.toml`, `setup.py`, `requirements.txt`).
    Python,
    /// Go project (`go.mod`).
    Go,
    /// Java project (`pom.xml`, `build.gradle`).
    Java,
    /// Ruby project (`Gemfile`).
    Ruby,
}

/// Git repository information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitInfo {
    /// Current branch name.
    pub branch: Option<String>,
    /// Summary of working tree status (e.g., "3 modified, 1 untracked").
    pub status_summary: Option<String>,
}

/// Context for AI requests (completion, chat, analysis).
#[derive(Debug, Clone, Default)]
pub struct AiContext {
    /// Current working directory, if known.
    pub cwd: Option<String>,
    /// Text the user has typed on the current line (after the prompt).
    pub input_prefix: String,
    /// Recent terminal output lines for context.
    pub recent_output: Vec<String>,
    /// Current shell name, if known.
    pub shell: Option<String>,
    /// Operating system, if known.
    pub os: Option<String>,
    /// Current git branch, if known. Prefer `git_info.branch` for new code.
    /// Kept for backward compatibility with callers that set only this field.
    pub git_branch: Option<String>,
    /// Detailed git repository information.
    pub git_info: Option<GitInfo>,
    /// Detected project type.
    pub project_type: Option<ProjectType>,
    /// Recent command history.
    pub command_history: Vec<CommandRecord>,
    /// Filtered environment variable hints.
    pub env_hints: Vec<(String, String)>,
}

/// Error context for AI error analysis.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// The command that failed.
    pub command: String,
    /// Exit code of the failed command.
    pub exit_code: i32,
    /// Standard error output.
    pub stderr: String,
    /// Standard output.
    pub stdout: String,
    /// Working directory when the command was run.
    pub cwd: Option<String>,
}

impl AiContext {
    /// Format a completion prompt from this context.
    ///
    /// Includes cwd, shell, os, git branch, recent output, and the current input prefix.
    /// This is the canonical prompt used by all providers.
    pub fn format_completion_prompt(&self) -> String {
        let cwd = self.cwd.as_deref().unwrap_or("unknown");
        let shell = self.shell.as_deref().unwrap_or("unknown");
        let os = self.os.as_deref().unwrap_or("unknown");

        let recent = if self.recent_output.is_empty() {
            "(none)".to_string()
        } else {
            self.recent_output.join("\n")
        };

        // Git info: prefer detailed git_info, fall back to git_branch.
        let git_section = if let Some(ref info) = self.git_info {
            let mut parts = Vec::new();
            if let Some(ref branch) = info.branch {
                parts.push(format!("Git branch: {branch}"));
            }
            if let Some(ref status) = info.status_summary {
                parts.push(format!("Git status: {status}"));
            }
            if parts.is_empty() {
                String::new()
            } else {
                format!("\n{}", parts.join("\n"))
            }
        } else {
            self.git_branch
                .as_deref()
                .map(|b| format!("\nGit branch: {b}"))
                .unwrap_or_default()
        };

        let project_section = self
            .project_type
            .as_ref()
            .map(|pt| format!("\nProject type: {pt:?}"))
            .unwrap_or_default();

        let env_section = if self.env_hints.is_empty() {
            String::new()
        } else {
            let vars: Vec<String> = self
                .env_hints
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            format!("\nEnvironment: {}", vars.join(", "))
        };

        let history_section = if self.command_history.is_empty() {
            String::new()
        } else {
            let cmds: Vec<String> = self
                .command_history
                .iter()
                .rev()
                .take(5)
                .map(|r| {
                    if r.exit_code == 0 {
                        format!("$ {}", r.command)
                    } else {
                        format!("$ {} (exit {})", r.command, r.exit_code)
                    }
                })
                .collect();
            format!("\nRecent commands:\n{}", cmds.join("\n"))
        };

        format!(
            "Complete the following terminal command. \
             Output only the completion text, nothing else.\n\n\
             Context:\n\
             OS: {os}\n\
             Shell: {shell}\n\
             CWD: {cwd}{git_section}{project_section}{env_section}{history_section}\n\
             Recent output:\n{recent}\n\n\
             Command to complete: {}",
            self.input_prefix
        )
    }
}

impl ErrorContext {
    /// Format an error analysis prompt from this context.
    ///
    /// Produces a prompt asking for a JSON object with `explanation`, `suggestions`,
    /// and `confidence` fields. This is the canonical prompt used by all providers.
    pub fn format_error_analysis_prompt(&self) -> String {
        let cwd = self.cwd.as_deref().unwrap_or("unknown");

        let stderr_section = if self.stderr.is_empty() {
            "(none)".to_string()
        } else {
            self.stderr.clone()
        };

        let stdout_section = if self.stdout.is_empty() {
            "(none)".to_string()
        } else {
            self.stdout.clone()
        };

        format!(
            "Analyze the following terminal command failure and provide:\n\
             1. A brief explanation of what went wrong.\n\
             2. Two or three concrete suggestions to fix it.\n\
             3. A confidence score from 0.0 to 1.0.\n\n\
             Respond in this exact JSON format:\n\
             {{\"explanation\": \"...\", \"suggestions\": [\"...\", \"...\"], \"confidence\": 0.9}}\n\n\
             Command: {command}\n\
             Exit code: {exit_code}\n\
             CWD: {cwd}\n\
             Stderr:\n{stderr_section}\n\
             Stdout:\n{stdout_section}",
            command = self.command,
            exit_code = self.exit_code,
        )
    }
}

/// Result of AI error analysis.
#[derive(Debug, Clone)]
pub struct ErrorAnalysis {
    /// Human-readable explanation of the error.
    pub explanation: String,
    /// Suggested fixes or next steps.
    pub suggestions: Vec<String>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
}
