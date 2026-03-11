//! Keybinding configuration.

use serde::Deserialize;

/// A single key binding.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Keybinding {
    /// The key to bind.
    pub key: String,
    /// Modifier keys (e.g. "ctrl", "shift", "alt").
    #[serde(default)]
    pub modifiers: Vec<String>,
    /// The action to perform.
    pub action: String,
}

/// Keybinding configuration section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct KeybindConfig {
    /// List of key bindings.
    pub bindings: Vec<Keybinding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_empty() {
        let kb = KeybindConfig::default();
        assert!(kb.bindings.is_empty());
    }

    #[test]
    fn test_parse_binding() {
        let toml_str = r#"
[[bindings]]
key = "c"
modifiers = ["ctrl"]
action = "copy"
"#;
        let kb: KeybindConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(kb.bindings.len(), 1);
        assert_eq!(kb.bindings[0].key, "c");
        assert_eq!(kb.bindings[0].action, "copy");
    }
}
