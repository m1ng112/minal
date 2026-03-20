//! `minal-config` -- Configuration management.
//!
//! Provides TOML-based configuration loading, theme definitions,
//! font settings, keybindings, and AI settings.
//!
//! Configuration is loaded from `~/.config/minal/minal.toml` (or the
//! platform-appropriate config directory). Missing fields gracefully
//! fall back to defaults (Catppuccin Mocha theme, JetBrains Mono font, etc.).

mod ai;
pub mod clipboard;
mod error;
mod font;
mod keybind;
mod theme;

pub use ai::{AiConfig, AiProvider};
pub use clipboard::ClipboardConfig;
pub use error::ConfigError;
pub use font::FontConfig;
pub use keybind::{Keybind, KeybindAction, KeybindConfig};
pub use theme::{AnsiColors, ThemeConfig, ThemePreset, builtin_theme};

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Window geometry and appearance settings.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct WindowConfig {
    /// Window width in columns.
    pub width: u32,
    /// Window height in rows.
    pub height: u32,
    /// Window opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub opacity: f32,
    /// Padding in pixels around the terminal content.
    pub padding: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 80,
            height: 24,
            opacity: 1.0,
            padding: 10,
        }
    }
}

impl WindowConfig {
    /// Validates the window configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is out of range.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.width == 0 {
            return Err(ConfigError::Validation(
                "window.width must be > 0".to_string(),
            ));
        }
        if self.height == 0 {
            return Err(ConfigError::Validation(
                "window.height must be > 0".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.opacity) {
            return Err(ConfigError::Validation(format!(
                "window.opacity must be between 0.0 and 1.0, got {}",
                self.opacity
            )));
        }
        Ok(())
    }
}

/// Shell program settings.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ShellConfig {
    /// Shell program path. `None` means auto-detect from `$SHELL`.
    pub program: Option<String>,
    /// Arguments passed to the shell program.
    pub args: Vec<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            program: None,
            args: vec!["-l".to_string()],
        }
    }
}

impl ShellConfig {
    /// Returns the shell program to use.
    ///
    /// Resolution order:
    /// 1. Explicit `program` value from config
    /// 2. `$SHELL` environment variable
    /// 3. `/bin/sh` as fallback
    pub fn resolve_program(&self) -> String {
        if let Some(ref prog) = self.program {
            return prog.clone();
        }
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Controls which Option key(s) are treated as the Alt modifier for terminal use.
///
/// On macOS the Option key can generate special characters (e.g. `∑` for `⌥W`).
/// For terminal applications it is usually more useful to have Option produce
/// ANSI Alt-escape sequences instead.  This setting lets users choose which
/// Option key(s) behave that way.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum OptionAsAlt {
    /// Only the left Option key generates Alt sequences (default).
    #[default]
    Left,
    /// Only the right Option key generates Alt sequences.
    Right,
    /// Both Option keys generate Alt sequences.
    Both,
    /// Neither Option key generates Alt sequences; both produce macOS glyphs.
    None,
}

/// macOS-specific settings.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct MacosConfig {
    /// Follow the system dark/light mode preference and switch themes automatically.
    pub follow_system_theme: bool,
    /// Which Option key(s) to treat as Alt for terminal use.
    pub option_as_alt: OptionAsAlt,
}

impl Default for MacosConfig {
    fn default() -> Self {
        Self {
            follow_system_theme: true,
            option_as_alt: OptionAsAlt::default(),
        }
    }
}

/// Root configuration for the Minal terminal emulator.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Font settings.
    pub font: FontConfig,
    /// Window settings.
    pub window: WindowConfig,
    /// Color theme settings.
    pub colors: ThemeConfig,
    /// Light theme override; applied when the system is in light mode and
    /// `macos.follow_system_theme` is `true`.
    pub colors_light: Option<ThemeConfig>,
    /// Shell settings.
    pub shell: ShellConfig,
    /// Keybinding settings.
    pub keybinds: KeybindConfig,
    /// AI feature settings.
    pub ai: AiConfig,
    /// Clipboard settings.
    pub clipboard: ClipboardConfig,
    /// macOS-specific settings.
    pub macos: MacosConfig,
}

impl Config {
    /// Returns the default configuration file path.
    ///
    /// On macOS: `~/Library/Application Support/minal/minal.toml`
    /// On Linux: `~/.config/minal/minal.toml`
    ///
    /// # Errors
    /// Returns `ConfigError::ConfigDir` if the config directory cannot be determined.
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            tracing::error!("could not determine config directory");
            ConfigError::ConfigDir
        })?;
        Ok(config_dir.join("minal").join("minal.toml"))
    }

    /// Loads configuration from the default config file path.
    ///
    /// If the file does not exist, returns default configuration.
    /// If the file exists but contains only partial settings, missing
    /// fields are filled with defaults.
    ///
    /// # Errors
    /// Returns `ConfigError` on I/O errors (other than not-found) or parse errors.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        Self::load_from(&path)
    }

    /// Loads configuration from a specific file path.
    ///
    /// If the file does not exist, returns default configuration.
    ///
    /// # Errors
    /// Returns `ConfigError` on I/O errors (other than not-found) or parse errors.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let config = Self::load_from_str(&contents)?;
                tracing::info!(?path, "loaded configuration file");
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(?path, "config file not found, using defaults");
                Ok(Self::default())
            }
            Err(e) => Err(ConfigError::Io(e)),
        }
    }

    /// Parses configuration from a TOML string.
    ///
    /// Missing fields are filled with defaults thanks to `#[serde(default)]`.
    ///
    /// # Errors
    /// Returns `ConfigError::Parse` if the TOML is malformed.
    pub fn load_from_str(s: &str) -> Result<Self, ConfigError> {
        let mut config: Self = toml::from_str(s)?;
        // Resolve theme preset: replaces color fields with built-in values
        // when a non-Custom preset is selected.
        config.colors = config.colors.resolve();
        if let Some(light) = config.colors_light.take() {
            config.colors_light = Some(light.resolve());
        }
        config.validate()?;
        Ok(config)
    }

    /// Validates all configuration values.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` describing the first invalid value found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.font.validate()?;
        self.window.validate()?;
        self.colors.validate()?;
        if let Some(ref light) = self.colors_light {
            light.validate()?;
        }
        self.ai.validate()?;
        self.keybinds.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        let cfg = Config::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn load_from_str_complete() {
        let toml_str = r##"
            [font]
            family = "Fira Code"
            size = 16.0
            line_height = 20.0

            [window]
            width = 120
            height = 40
            opacity = 0.95
            padding = 8

            [colors]
            background = "#000000"
            foreground = "#ffffff"

            [colors.ansi]
            black = "#111111"
            red = "#ff0000"
            green = "#00ff00"
            yellow = "#ffff00"
            blue = "#0000ff"
            magenta = "#ff00ff"
            cyan = "#00ffff"
            white = "#cccccc"
            bright_black = "#333333"
            bright_red = "#ff3333"
            bright_green = "#33ff33"
            bright_yellow = "#ffff33"
            bright_blue = "#3333ff"
            bright_magenta = "#ff33ff"
            bright_cyan = "#33ffff"
            bright_white = "#ffffff"

            [shell]
            program = "/bin/zsh"
            args = ["-l", "--login"]

            [keybinds]
            bindings = []

            [ai]
            provider = "anthropic"
            enabled = false
            model = "claude-3-haiku"
        "##;
        let cfg = Config::load_from_str(toml_str).unwrap();
        assert_eq!(cfg.font.family, "Fira Code");
        assert_eq!(cfg.window.width, 120);
        assert_eq!(cfg.colors.background, "#000000");
        assert_eq!(cfg.shell.program, Some("/bin/zsh".to_string()));
        assert_eq!(cfg.ai.provider, AiProvider::Anthropic);
        assert!(!cfg.ai.enabled);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn load_from_str_partial_font_only() {
        let toml_str = r#"
            [font]
            size = 20.0
        "#;
        let cfg = Config::load_from_str(toml_str).unwrap();
        assert!((cfg.font.size - 20.0).abs() < f32::EPSILON);
        // Everything else should be defaults
        assert_eq!(cfg.font.family, "JetBrains Mono");
        assert_eq!(cfg.window, WindowConfig::default());
        assert_eq!(cfg.colors, ThemeConfig::default());
        assert_eq!(cfg.shell, ShellConfig::default());
        assert_eq!(cfg.ai, AiConfig::default());
    }

    #[test]
    fn load_from_str_empty() {
        let cfg = Config::load_from_str("").unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn load_from_nonexistent_returns_defaults() {
        let path = Path::new("/tmp/minal_test_nonexistent_config_file.toml");
        let cfg = Config::load_from(path).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn validate_catches_invalid_font_size() {
        let mut cfg = Config::default();
        cfg.font.size = 0.0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_catches_invalid_window_opacity() {
        let mut cfg = Config::default();
        cfg.window.opacity = 1.5;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_catches_invalid_color() {
        let mut cfg = Config::default();
        cfg.colors.background = "not-hex".to_string();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_catches_zero_window_width() {
        let mut cfg = Config::default();
        cfg.window.width = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: Config = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn shell_resolve_program_explicit() {
        let cfg = ShellConfig {
            program: Some("/usr/local/bin/fish".to_string()),
            args: vec![],
        };
        assert_eq!(cfg.resolve_program(), "/usr/local/bin/fish");
    }

    #[test]
    fn shell_resolve_program_from_env() {
        // $SHELL is typically set on macOS/Linux CI
        let cfg = ShellConfig::default();
        let resolved = cfg.resolve_program();
        assert!(!resolved.is_empty());
    }

    #[test]
    fn config_path_is_valid() {
        // This should succeed on any platform with a home directory
        let path = Config::config_path().unwrap();
        assert!(path.ends_with("minal/minal.toml"));
    }

    #[test]
    fn window_config_defaults() {
        let cfg = WindowConfig::default();
        assert_eq!(cfg.width, 80);
        assert_eq!(cfg.height, 24);
        assert!((cfg.opacity - 1.0).abs() < f32::EPSILON);
        assert_eq!(cfg.padding, 10);
    }

    #[test]
    fn shell_config_defaults() {
        let cfg = ShellConfig::default();
        assert_eq!(cfg.program, None);
        assert_eq!(cfg.args, vec!["-l".to_string()]);
    }

    #[test]
    fn load_from_str_with_unknown_fields_is_ok() {
        // toml/serde ignores unknown fields by default (no deny_unknown_fields)
        let toml_str = r#"
            [font]
            family = "Menlo"
            unknown_field = "should be ignored"
        "#;
        let cfg =
            Config::load_from_str(toml_str).expect("unknown fields should be silently ignored");
        assert_eq!(cfg.font.family, "Menlo");
    }

    #[test]
    fn macos_config_defaults() {
        let cfg = MacosConfig::default();
        assert!(cfg.follow_system_theme);
        assert_eq!(cfg.option_as_alt, OptionAsAlt::Left);
    }

    #[test]
    fn colors_light_parsed() {
        let toml_str = "[colors_light]\nbackground = \"#ffffff\"\nforeground = \"#000000\"\n";
        let cfg = Config::load_from_str(toml_str).unwrap();
        assert!(cfg.colors_light.is_some());
    }

    #[test]
    fn colors_light_invalid_color_fails_validation() {
        let mut cfg = Config::default();
        let mut light = ThemeConfig::default();
        light.background = "not-hex".to_string();
        cfg.colors_light = Some(light);
        assert!(cfg.validate().is_err());
    }
}
