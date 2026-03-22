//! Keybinding configuration.

use serde::{Deserialize, Serialize};

/// Actions that can be triggered by key bindings.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum KeybindAction {
    /// Copy selection to clipboard.
    Copy,
    /// Paste from clipboard.
    Paste,
    /// Open a new tab.
    NewTab,
    /// Close the current tab.
    CloseTab,
    /// Close the focused pane; if last pane in tab, close the tab.
    ClosePaneOrTab,
    /// Switch to the next tab.
    NextTab,
    /// Switch to the previous tab.
    PrevTab,
    /// Switch to a specific tab by number (1-9).
    SwitchTab(u8),
    /// Split the current pane vertically (side by side).
    SplitVertical,
    /// Split the current pane horizontally (top and bottom).
    SplitHorizontal,
    /// Move focus to the next pane.
    FocusNextPane,
    /// Move focus to the previous pane.
    FocusPrevPane,
    /// Increase font size.
    IncreaseFontSize,
    /// Decrease font size.
    DecreaseFontSize,
    /// Reset font size to default.
    ResetFontSize,
    /// Accept AI completion suggestion.
    AiAcceptCompletion,
    /// Dismiss AI completion suggestion.
    AiDismissCompletion,
    /// Toggle AI features on/off.
    AiToggle,
    /// Toggle the inline AI chat panel.
    AiToggleChat,
    /// Toggle the error analysis panel.
    AiToggleErrorPanel,
    /// Toggle the agent mode panel.
    AiToggleAgent,
    /// Toggle MCP tools on/off.
    AiToggleMcpTools,
    /// A user-defined custom action.
    Custom(String),
}

/// A single key binding entry.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Keybind {
    /// The key name (e.g. "c", "v", "t", "F1").
    pub key: String,
    /// Modifier keys (e.g. "Ctrl", "Shift", "Super").
    pub modifiers: Vec<String>,
    /// The action to perform when the key combination is pressed.
    pub action: KeybindAction,
}

/// Configuration for key bindings.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct KeybindConfig {
    /// List of key bindings.
    pub bindings: Vec<Keybind>,
}

impl KeybindConfig {
    /// Returns the default keybindings for macOS.
    ///
    /// Includes tab/pane management, clipboard, and tab switching.
    pub fn default_macos() -> Self {
        Self {
            bindings: vec![
                Keybind {
                    key: "c".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::Copy,
                },
                Keybind {
                    key: "v".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::Paste,
                },
                Keybind {
                    key: "t".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::NewTab,
                },
                Keybind {
                    key: "w".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::ClosePaneOrTab,
                },
                Keybind {
                    key: "d".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SplitVertical,
                },
                Keybind {
                    key: "d".to_string(),
                    modifiers: vec!["Super".to_string(), "Shift".to_string()],
                    action: KeybindAction::SplitHorizontal,
                },
                Keybind {
                    key: "]".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::FocusNextPane,
                },
                Keybind {
                    key: "[".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::FocusPrevPane,
                },
                Keybind {
                    key: "1".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(1),
                },
                Keybind {
                    key: "2".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(2),
                },
                Keybind {
                    key: "3".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(3),
                },
                Keybind {
                    key: "4".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(4),
                },
                Keybind {
                    key: "5".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(5),
                },
                Keybind {
                    key: "6".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(6),
                },
                Keybind {
                    key: "7".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(7),
                },
                Keybind {
                    key: "8".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(8),
                },
                Keybind {
                    key: "9".to_string(),
                    modifiers: vec!["Super".to_string()],
                    action: KeybindAction::SwitchTab(9),
                },
                Keybind {
                    key: "a".to_string(),
                    modifiers: vec!["Control".to_string(), "Shift".to_string()],
                    action: KeybindAction::AiToggleChat,
                },
                Keybind {
                    key: "e".to_string(),
                    modifiers: vec!["Control".to_string(), "Shift".to_string()],
                    action: KeybindAction::AiToggleErrorPanel,
                },
                Keybind {
                    key: "g".to_string(),
                    modifiers: vec!["Control".to_string(), "Shift".to_string()],
                    action: KeybindAction::AiToggleAgent,
                },
                Keybind {
                    key: "m".to_string(),
                    modifiers: vec!["Control".to_string(), "Shift".to_string()],
                    action: KeybindAction::AiToggleMcpTools,
                },
            ],
        }
    }

    /// Validates the keybinding configuration.
    ///
    /// Currently a no-op; reserved for future validation
    /// (e.g. modifier name checking).
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_empty() {
        let cfg = KeybindConfig::default();
        assert!(cfg.bindings.is_empty());
    }

    #[test]
    fn deserialize_bindings() {
        let toml_str = r#"
            [[bindings]]
            key = "c"
            modifiers = ["Super"]
            action = "Copy"

            [[bindings]]
            key = "v"
            modifiers = ["Super"]
            action = "Paste"
        "#;
        let cfg: KeybindConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.bindings.len(), 2);
        assert_eq!(cfg.bindings[0].action, KeybindAction::Copy);
        assert_eq!(cfg.bindings[1].action, KeybindAction::Paste);
        assert_eq!(cfg.bindings[0].modifiers, vec!["Super".to_string()]);
    }

    #[test]
    fn deserialize_custom_action() {
        let toml_str = r#"
            [[bindings]]
            key = "F1"
            modifiers = []
            action = { Custom = "toggle-ai" }
        "#;
        let cfg: KeybindConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.bindings.len(), 1);
        assert_eq!(
            cfg.bindings[0].action,
            KeybindAction::Custom("toggle-ai".to_string())
        );
    }

    #[test]
    fn deserialize_empty() {
        let cfg: KeybindConfig = toml::from_str("").unwrap();
        assert_eq!(cfg, KeybindConfig::default());
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = KeybindConfig {
            bindings: vec![Keybind {
                key: "t".to_string(),
                modifiers: vec!["Super".to_string()],
                action: KeybindAction::NewTab,
            }],
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: KeybindConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn serialize_roundtrip_switch_tab() {
        let cfg = KeybindConfig {
            bindings: vec![Keybind {
                key: "1".to_string(),
                modifiers: vec!["Super".to_string()],
                action: KeybindAction::SwitchTab(1),
            }],
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: KeybindConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn serialize_roundtrip_pane_actions() {
        let actions = vec![
            KeybindAction::SplitVertical,
            KeybindAction::SplitHorizontal,
            KeybindAction::FocusNextPane,
            KeybindAction::FocusPrevPane,
            KeybindAction::ClosePaneOrTab,
        ];
        for action in actions {
            let cfg = KeybindConfig {
                bindings: vec![Keybind {
                    key: "x".to_string(),
                    modifiers: vec![],
                    action: action.clone(),
                }],
            };
            let s = toml::to_string(&cfg).unwrap();
            let cfg2: KeybindConfig = toml::from_str(&s).unwrap();
            assert_eq!(cfg, cfg2, "round-trip failed for {action:?}");
        }
    }

    #[test]
    fn default_macos_keybinds() {
        let cfg = KeybindConfig::default_macos();
        assert!(!cfg.bindings.is_empty());
        // Verify key actions exist
        assert!(
            cfg.bindings
                .iter()
                .any(|b| b.action == KeybindAction::NewTab)
        );
        assert!(
            cfg.bindings
                .iter()
                .any(|b| b.action == KeybindAction::SplitVertical)
        );
        assert!(
            cfg.bindings
                .iter()
                .any(|b| b.action == KeybindAction::SwitchTab(1))
        );
    }
}
