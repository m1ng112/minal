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
    /// Current git branch, if known.
    pub git_branch: Option<String>,
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

        let git_info = self
            .git_branch
            .as_deref()
            .map(|b| format!("\nGit branch: {b}"))
            .unwrap_or_default();

        format!(
            "Complete the following terminal command. \
             Output only the completion text, nothing else.\n\n\
             Context:\n\
             OS: {os}\n\
             Shell: {shell}\n\
             CWD: {cwd}{git_info}\n\
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
