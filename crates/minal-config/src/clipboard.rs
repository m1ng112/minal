use serde::{Deserialize, Serialize};

/// Clipboard configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipboardConfig {
    /// Automatically copy text to clipboard when selection is made.
    #[serde(default)]
    pub auto_copy_on_select: bool,
    /// Allow OSC 52 clipboard read operations from programs.
    /// Disabled by default for security (prevents silent clipboard exfiltration).
    #[serde(default)]
    pub osc52_read: bool,
    /// Allow OSC 52 clipboard write operations from programs.
    /// Enabled by default (matches Alacritty/iTerm2 convention).
    /// Note: programs in the terminal can silently overwrite the system clipboard.
    #[serde(default = "default_true")]
    pub osc52_write: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            auto_copy_on_select: false,
            osc52_read: false,
            osc52_write: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = ClipboardConfig::default();
        assert!(!config.auto_copy_on_select);
        assert!(!config.osc52_read);
        assert!(config.osc52_write);
    }

    #[test]
    fn test_deserialize_full() {
        let toml_str = r#"
            auto_copy_on_select = true
            osc52_read = true
            osc52_write = false
        "#;
        let config: ClipboardConfig = toml::from_str(toml_str).unwrap();
        assert!(config.auto_copy_on_select);
        assert!(config.osc52_read);
        assert!(!config.osc52_write);
    }

    #[test]
    fn test_deserialize_partial() {
        let toml_str = r#"
            auto_copy_on_select = true
        "#;
        let config: ClipboardConfig = toml::from_str(toml_str).unwrap();
        assert!(config.auto_copy_on_select);
        assert!(!config.osc52_read);
        assert!(config.osc52_write);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let config = ClipboardConfig {
            auto_copy_on_select: true,
            osc52_read: true,
            osc52_write: false,
        };
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: ClipboardConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }
}
