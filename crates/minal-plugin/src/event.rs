//! Plugin event types and hook registry.
//!
//! Defines the events that plugins can subscribe to via their manifest.
//! The [`HookRegistry`] tracks which plugins are interested in which events
//! and provides efficient dispatch.

use serde::{Deserialize, Serialize};

/// Events that can be dispatched to plugins.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginEvent {
    /// A shell command is about to execute or has been entered.
    #[serde(rename = "command")]
    Command {
        /// The command string.
        command: String,
        /// Current working directory.
        working_dir: String,
    },
    /// Terminal output was received.
    #[serde(rename = "output")]
    Output {
        /// The output data (may be partial).
        data: String,
    },
    /// A command exited with a non-zero status.
    #[serde(rename = "error")]
    Error {
        /// The command that failed.
        command: String,
        /// Exit code of the command.
        exit_code: i32,
        /// Standard error output.
        stderr: String,
    },
}

/// Response from a plugin hook invocation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookResponse {
    /// If `true`, the event should be suppressed (not processed further).
    #[serde(default)]
    pub suppress: bool,
    /// Optional modified command (only meaningful for `Command` events).
    #[serde(default)]
    pub modified_command: Option<String>,
    /// Optional message to display to the user.
    #[serde(default)]
    pub message: Option<String>,
}

/// Tracks which plugin indices are registered for each event type.
#[derive(Debug, Default)]
pub struct HookRegistry {
    /// Plugin indices that subscribe to `on_command`.
    pub command_hooks: Vec<usize>,
    /// Plugin indices that subscribe to `on_output`.
    pub output_hooks: Vec<usize>,
    /// Plugin indices that subscribe to `on_error`.
    pub error_hooks: Vec<usize>,
}

impl HookRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin (by index) for its declared hooks.
    pub fn register(&mut self, index: usize, on_command: bool, on_output: bool, on_error: bool) {
        if on_command {
            self.command_hooks.push(index);
        }
        if on_output {
            self.output_hooks.push(index);
        }
        if on_error {
            self.error_hooks.push(index);
        }
    }

    /// Remove all hook registrations for a plugin index.
    pub fn unregister(&mut self, index: usize) {
        self.command_hooks.retain(|&i| i != index);
        self.output_hooks.retain(|&i| i != index);
        self.error_hooks.retain(|&i| i != index);
    }

    /// Return the plugin indices that should receive a given event.
    pub fn subscribers(&self, event: &PluginEvent) -> &[usize] {
        match event {
            PluginEvent::Command { .. } => &self.command_hooks,
            PluginEvent::Output { .. } => &self.output_hooks,
            PluginEvent::Error { .. } => &self.error_hooks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_registry_register_and_lookup() {
        let mut reg = HookRegistry::new();
        reg.register(0, true, false, true);
        reg.register(1, false, true, false);
        reg.register(2, true, true, true);

        let cmd_event = PluginEvent::Command {
            command: "ls".to_string(),
            working_dir: "/tmp".to_string(),
        };
        assert_eq!(reg.subscribers(&cmd_event), &[0, 2]);

        let out_event = PluginEvent::Output {
            data: "hello".to_string(),
        };
        assert_eq!(reg.subscribers(&out_event), &[1, 2]);

        let err_event = PluginEvent::Error {
            command: "false".to_string(),
            exit_code: 1,
            stderr: String::new(),
        };
        assert_eq!(reg.subscribers(&err_event), &[0, 2]);
    }

    #[test]
    fn hook_registry_unregister() {
        let mut reg = HookRegistry::new();
        reg.register(0, true, true, true);
        reg.register(1, true, true, true);
        reg.unregister(0);

        let cmd_event = PluginEvent::Command {
            command: "ls".to_string(),
            working_dir: "/tmp".to_string(),
        };
        assert_eq!(reg.subscribers(&cmd_event), &[1]);
    }

    #[test]
    fn plugin_event_serialization_roundtrip() {
        let event = PluginEvent::Command {
            command: "cargo build".to_string(),
            working_dir: "/home/user/project".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let parsed: PluginEvent = serde_json::from_str(&json).expect("deserialize");
        match parsed {
            PluginEvent::Command {
                command,
                working_dir,
            } => {
                assert_eq!(command, "cargo build");
                assert_eq!(working_dir, "/home/user/project");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn hook_response_defaults() {
        let resp = HookResponse::default();
        assert!(!resp.suppress);
        assert!(resp.modified_command.is_none());
        assert!(resp.message.is_none());
    }
}
