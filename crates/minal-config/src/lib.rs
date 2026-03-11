//! `minal-config` — Configuration management.
//!
//! Loads settings from `~/.config/minal/minal.toml` and provides
//! typed configuration with sensible defaults.

pub mod ai;
mod error;
pub mod font;
pub mod keybind;
pub mod shell;
pub mod theme;
pub mod window;

pub use ai::AiConfig;
pub use error::ConfigError;
pub use font::FontConfig;
pub use keybind::{KeybindConfig, Keybinding};
pub use shell::ShellConfig;
pub use theme::ThemeConfig;
pub use window::WindowConfig;

use serde::Deserialize;
use std::path::PathBuf;

/// Top-level configuration.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Font settings.
    pub font: FontConfig,
    /// Window settings.
    pub window: WindowConfig,
    /// Shell settings.
    pub shell: ShellConfig,
    /// Color theme settings.
    pub colors: ThemeConfig,
    /// Keybinding settings.
    pub keybindings: KeybindConfig,
    /// AI integration settings.
    pub ai: AiConfig,
}

impl Config {
    /// Returns the default configuration file path.
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("minal").join("minal.toml"))
    }

    /// Load configuration from the default file path.
    ///
    /// Returns defaults if the file does not exist.
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load() -> Result<Self, ConfigError> {
        let Some(path) = Self::config_path() else {
            tracing::info!("No config directory found, using defaults");
            return Ok(Self::default());
        };

        if !path.exists() {
            tracing::info!(
                "Config file not found at {}, using defaults",
                path.display()
            );
            return Ok(Self::default());
        }

        tracing::info!("Loading config from {}", path.display());
        let contents = std::fs::read_to_string(&path)?;
        Self::load_from_str(&contents)
    }

    /// Load configuration from a TOML string.
    pub fn load_from_str(s: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(s)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string_defaults() {
        let config = Config::load_from_str("").unwrap();
        assert_eq!(config.font.family, "JetBrains Mono");
        assert_eq!(config.window.columns, 80);
        assert!(config.ai.enabled);
    }

    #[test]
    fn test_full_config() {
        let toml_str = r##"
[font]
family = "Fira Code"
size = 16.0

[window]
columns = 120
rows = 40

[colors]
background = "#000000"
foreground = "#ffffff"

[shell]
program = "/bin/bash"
args = ["-l"]

[ai]
provider = "anthropic"
model = "claude-3-haiku"
enabled = false
"##;
        let config = Config::load_from_str(toml_str).unwrap();
        assert_eq!(config.font.family, "Fira Code");
        assert_eq!(config.font.size, 16.0);
        assert_eq!(config.window.columns, 120);
        assert_eq!(config.colors.background, "#000000");
        assert_eq!(config.shell.program, "/bin/bash");
        assert!(!config.ai.enabled);
        assert_eq!(config.ai.provider, "anthropic");
    }

    #[test]
    fn test_partial_config() {
        let toml_str = r#"
[font]
size = 18.0
"#;
        let config = Config::load_from_str(toml_str).unwrap();
        assert_eq!(config.font.size, 18.0);
        assert_eq!(config.font.family, "JetBrains Mono"); // default
        assert_eq!(config.window.columns, 80); // default
    }

    #[test]
    fn test_malformed_toml_error() {
        let result = Config::load_from_str("invalid [[ toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_returns_defaults_when_no_file() {
        // Config::load() should not panic even without a config file
        let config = Config::load().unwrap();
        assert_eq!(config.font.family, "JetBrains Mono");
    }
}
