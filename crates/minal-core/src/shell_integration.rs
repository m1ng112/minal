//! Shell integration via OSC 133 semantic prompt protocol.
//!
//! Tracks shell prompt state transitions (A→B→C→D) and produces
//! structured [`ShellCommandRecord`] values for AI context.

use std::time::{SystemTime, UNIX_EPOCH};

/// State of the shell prompt lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptState {
    /// No shell integration markers received yet, or after command completion.
    Idle,
    /// OSC 133;A received – the prompt is being drawn.
    PromptActive,
    /// OSC 133;B received – the cursor is in the command input area.
    CommandInput {
        /// Grid row where command input starts.
        input_start_row: usize,
        /// Grid column where command input starts.
        input_start_col: usize,
    },
    /// OSC 133;C received – the shell is executing a command.
    Executing {
        /// The command text extracted from the grid.
        command: String,
        /// Row where command output begins (for output capture).
        output_start_row: usize,
        /// Unix timestamp (seconds) when execution started.
        start_time: u64,
    },
}

/// A structured record of a completed shell command.
///
/// This mirrors `minal_ai::CommandRecord` fields but lives in `minal-core`
/// to avoid a circular crate dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommandRecord {
    /// The command that was executed.
    pub command: String,
    /// Truncated output of the command.
    pub output: String,
    /// Exit code of the command.
    pub exit_code: i32,
    /// Unix timestamp (seconds) when the command started.
    pub timestamp: u64,
}

/// Events produced by the shell integration state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    /// A command completed with a structured record.
    CommandCompleted(ShellCommandRecord),
    /// A new prompt started (OSC 133;A). Can be used for prompt mark rendering.
    PromptStarted,
}

/// Shell integration state tracker for OSC 133.
///
/// Consumes prompt lifecycle transitions and produces [`ShellEvent`] values.
#[derive(Debug)]
pub struct ShellIntegration {
    /// Current prompt lifecycle state.
    state: PromptState,
}

impl Default for ShellIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellIntegration {
    /// Create a new shell integration tracker in the idle state.
    pub fn new() -> Self {
        Self {
            state: PromptState::Idle,
        }
    }

    /// Current prompt state.
    pub fn state(&self) -> &PromptState {
        &self.state
    }

    /// Handle OSC 133;A – prompt start.
    pub fn on_prompt_start(&mut self) {
        self.state = PromptState::PromptActive;
    }

    /// Handle OSC 133;B – command input area starts at the given grid position.
    pub fn on_command_input_start(&mut self, row: usize, col: usize) {
        // Only transition from PromptActive; ignore if out of order.
        if matches!(self.state, PromptState::PromptActive) {
            self.state = PromptState::CommandInput {
                input_start_row: row,
                input_start_col: col,
            };
        }
    }

    /// Handle OSC 133;C – command execution starts.
    ///
    /// `command` is the text extracted from the grid between the B-mark and the cursor.
    /// `cursor_row` is the current cursor row (output will start on the next row).
    pub fn on_command_execute(&mut self, command: String, cursor_row: usize) {
        // Accept from CommandInput or PromptActive (B may be skipped by some shells).
        if matches!(
            self.state,
            PromptState::CommandInput { .. } | PromptState::PromptActive
        ) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            self.state = PromptState::Executing {
                command,
                output_start_row: cursor_row,
                start_time: now,
            };
        }
    }

    /// Handle OSC 133;D – command completed with the given exit code.
    ///
    /// Returns a [`ShellEvent::CommandCompleted`] if we were in the Executing state.
    pub fn on_command_complete(&mut self, exit_code: i32, output: &str) -> Option<ShellEvent> {
        if let PromptState::Executing {
            ref command,
            start_time,
            ..
        } = self.state
        {
            let record = ShellCommandRecord {
                command: command.clone(),
                output: output.to_string(),
                exit_code,
                timestamp: start_time,
            };
            self.state = PromptState::Idle;
            Some(ShellEvent::CommandCompleted(record))
        } else {
            self.state = PromptState::Idle;
            None
        }
    }

    /// Reset to the idle state.
    pub fn reset(&mut self) {
        self.state = PromptState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_idle() {
        let si = ShellIntegration::new();
        assert_eq!(*si.state(), PromptState::Idle);
    }

    #[test]
    fn full_lifecycle() {
        let mut si = ShellIntegration::new();

        // A: prompt start
        si.on_prompt_start();
        assert_eq!(*si.state(), PromptState::PromptActive);

        // B: command input
        si.on_command_input_start(5, 2);
        assert!(matches!(
            si.state(),
            PromptState::CommandInput {
                input_start_row: 5,
                input_start_col: 2
            }
        ));

        // C: execute
        si.on_command_execute("ls -la".to_string(), 5);
        assert!(matches!(si.state(), PromptState::Executing { .. }));
        if let PromptState::Executing { ref command, .. } = si.state {
            assert_eq!(command, "ls -la");
        }

        // D: complete
        let event = si.on_command_complete(0, "file1\nfile2\n");
        assert_eq!(*si.state(), PromptState::Idle);
        let event = event.expect("should produce CommandCompleted");
        match event {
            ShellEvent::CommandCompleted(record) => {
                assert_eq!(record.command, "ls -la");
                assert_eq!(record.output, "file1\nfile2\n");
                assert_eq!(record.exit_code, 0);
                assert!(record.timestamp > 0);
            }
            _ => panic!("expected CommandCompleted"),
        }
    }

    #[test]
    fn d_without_c_returns_none() {
        let mut si = ShellIntegration::new();
        si.on_prompt_start();
        si.on_command_input_start(0, 0);
        // Skip C, go directly to D
        let event = si.on_command_complete(1, "");
        assert!(event.is_none());
        assert_eq!(*si.state(), PromptState::Idle);
    }

    #[test]
    fn b_without_a_ignored() {
        let mut si = ShellIntegration::new();
        // B without A should be ignored (state stays Idle)
        si.on_command_input_start(0, 0);
        assert_eq!(*si.state(), PromptState::Idle);
    }

    #[test]
    fn c_from_prompt_active_skipping_b() {
        let mut si = ShellIntegration::new();
        si.on_prompt_start();
        // Some shells may emit C without B
        si.on_command_execute("echo hello".to_string(), 1);
        assert!(matches!(si.state(), PromptState::Executing { .. }));
    }

    #[test]
    fn nonzero_exit_code() {
        let mut si = ShellIntegration::new();
        si.on_prompt_start();
        si.on_command_input_start(0, 0);
        si.on_command_execute("false".to_string(), 0);
        let event = si.on_command_complete(1, "");
        let event = event.expect("should produce CommandCompleted");
        match event {
            ShellEvent::CommandCompleted(record) => {
                assert_eq!(record.exit_code, 1);
            }
            _ => panic!("expected CommandCompleted"),
        }
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut si = ShellIntegration::new();
        si.on_prompt_start();
        si.on_command_input_start(0, 0);
        si.reset();
        assert_eq!(*si.state(), PromptState::Idle);
    }

    #[test]
    fn consecutive_commands() {
        let mut si = ShellIntegration::new();

        // First command
        si.on_prompt_start();
        si.on_command_input_start(0, 0);
        si.on_command_execute("cmd1".to_string(), 0);
        let e1 = si.on_command_complete(0, "out1");
        assert!(e1.is_some());

        // Second command
        si.on_prompt_start();
        si.on_command_input_start(2, 0);
        si.on_command_execute("cmd2".to_string(), 2);
        let e2 = si.on_command_complete(42, "out2");
        let e2 = e2.expect("should produce second CommandCompleted");
        match e2 {
            ShellEvent::CommandCompleted(record) => {
                assert_eq!(record.command, "cmd2");
                assert_eq!(record.exit_code, 42);
            }
            _ => panic!("expected CommandCompleted"),
        }
    }
}
