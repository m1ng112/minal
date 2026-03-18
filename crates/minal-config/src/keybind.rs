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
    /// Switch to the next tab.
    NextTab,
    /// Switch to the previous tab.
    PrevTab,
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
}
