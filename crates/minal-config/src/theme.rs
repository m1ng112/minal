//! Theme and color configuration.
//!
//! Default palette is Catppuccin Mocha.

use serde::{Deserialize, Serialize};

use crate::ConfigError;

/// Parse a hex color string like `#rrggbb` into `(r, g, b)`.
///
/// # Errors
/// Returns `ConfigError::Validation` if the string is not a valid hex color.
pub(crate) fn parse_hex_color(s: &str) -> Result<(u8, u8, u8), ConfigError> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return Err(ConfigError::Validation(format!(
            "hex color must be 6 hex digits (with optional #), got \"{s}\""
        )));
    }
    let r = u8::from_str_radix(&s[0..2], 16).map_err(|_| {
        ConfigError::Validation(format!("invalid hex digit in color red channel: \"{s}\""))
    })?;
    let g = u8::from_str_radix(&s[2..4], 16).map_err(|_| {
        ConfigError::Validation(format!("invalid hex digit in color green channel: \"{s}\""))
    })?;
    let b = u8::from_str_radix(&s[4..6], 16).map_err(|_| {
        ConfigError::Validation(format!("invalid hex digit in color blue channel: \"{s}\""))
    })?;
    Ok((r, g, b))
}

/// The 16 standard ANSI colors.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AnsiColors {
    /// ANSI color 0.
    pub black: String,
    /// ANSI color 1.
    pub red: String,
    /// ANSI color 2.
    pub green: String,
    /// ANSI color 3.
    pub yellow: String,
    /// ANSI color 4.
    pub blue: String,
    /// ANSI color 5.
    pub magenta: String,
    /// ANSI color 6.
    pub cyan: String,
    /// ANSI color 7.
    pub white: String,
    /// ANSI color 8.
    pub bright_black: String,
    /// ANSI color 9.
    pub bright_red: String,
    /// ANSI color 10.
    pub bright_green: String,
    /// ANSI color 11.
    pub bright_yellow: String,
    /// ANSI color 12.
    pub bright_blue: String,
    /// ANSI color 13.
    pub bright_magenta: String,
    /// ANSI color 14.
    pub bright_cyan: String,
    /// ANSI color 15.
    pub bright_white: String,
}

impl Default for AnsiColors {
    fn default() -> Self {
        // Catppuccin Mocha palette
        Self {
            black: "#45475a".to_string(),
            red: "#f38ba8".to_string(),
            green: "#a6e3a1".to_string(),
            yellow: "#f9e2af".to_string(),
            blue: "#89b4fa".to_string(),
            magenta: "#cba6f7".to_string(),
            cyan: "#94e2d3".to_string(),
            white: "#bac2de".to_string(),
            bright_black: "#585b70".to_string(),
            bright_red: "#f38ba8".to_string(),
            bright_green: "#a6e3a1".to_string(),
            bright_yellow: "#f9e2af".to_string(),
            bright_blue: "#89b4fa".to_string(),
            bright_magenta: "#cba6f7".to_string(),
            bright_cyan: "#94e2d3".to_string(),
            bright_white: "#cdd6f4".to_string(),
        }
    }
}

impl AnsiColors {
    /// Validates that all ANSI color values are valid hex colors.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any color is invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let colors = [
            ("black", &self.black),
            ("red", &self.red),
            ("green", &self.green),
            ("yellow", &self.yellow),
            ("blue", &self.blue),
            ("magenta", &self.magenta),
            ("cyan", &self.cyan),
            ("white", &self.white),
            ("bright_black", &self.bright_black),
            ("bright_red", &self.bright_red),
            ("bright_green", &self.bright_green),
            ("bright_yellow", &self.bright_yellow),
            ("bright_blue", &self.bright_blue),
            ("bright_magenta", &self.bright_magenta),
            ("bright_cyan", &self.bright_cyan),
            ("bright_white", &self.bright_white),
        ];
        for (name, value) in &colors {
            parse_hex_color(value).map_err(|_| {
                ConfigError::Validation(format!(
                    "colors.ansi.{name} is not a valid hex color: \"{value}\""
                ))
            })?;
        }
        Ok(())
    }
}

/// Theme/color configuration for the terminal.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ThemeConfig {
    /// Background color in hex (#rrggbb).
    pub background: String,
    /// Foreground (text) color in hex (#rrggbb).
    pub foreground: String,
    /// The 16 ANSI colors.
    pub ansi: AnsiColors,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        // Catppuccin Mocha
        Self {
            background: "#1e1e2e".to_string(),
            foreground: "#cdd6f4".to_string(),
            ansi: AnsiColors::default(),
        }
    }
}

impl ThemeConfig {
    /// Validates that all color values are valid hex colors.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any color is invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        parse_hex_color(&self.background).map_err(|_| {
            ConfigError::Validation(format!(
                "colors.background is not a valid hex color: \"{}\"",
                self.background
            ))
        })?;
        parse_hex_color(&self.foreground).map_err(|_| {
            ConfigError::Validation(format!(
                "colors.foreground is not a valid hex color: \"{}\"",
                self.foreground
            ))
        })?;
        self.ansi.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_valid_with_hash() {
        let (r, g, b) = parse_hex_color("#1e1e2e").unwrap();
        assert_eq!((r, g, b), (0x1e, 0x1e, 0x2e));
    }

    #[test]
    fn parse_hex_valid_without_hash() {
        let (r, g, b) = parse_hex_color("cdd6f4").unwrap();
        assert_eq!((r, g, b), (0xcd, 0xd6, 0xf4));
    }

    #[test]
    fn parse_hex_invalid_length() {
        assert!(parse_hex_color("#fff").is_err());
    }

    #[test]
    fn parse_hex_invalid_chars() {
        assert!(parse_hex_color("#gggggg").is_err());
    }

    #[test]
    fn parse_hex_empty() {
        assert!(parse_hex_color("").is_err());
    }

    #[test]
    fn default_values() {
        let cfg = ThemeConfig::default();
        assert_eq!(cfg.background, "#1e1e2e");
        assert_eq!(cfg.foreground, "#cdd6f4");
        assert_eq!(cfg.ansi.black, "#45475a");
        assert_eq!(cfg.ansi.bright_white, "#cdd6f4");
    }

    #[test]
    fn validate_valid() {
        let cfg = ThemeConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_invalid_background() {
        let cfg = ThemeConfig {
            background: "not-a-color".to_string(),
            ..ThemeConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_invalid_foreground() {
        let cfg = ThemeConfig {
            foreground: "xyz".to_string(),
            ..ThemeConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_invalid_ansi_color() {
        let mut cfg = ThemeConfig::default();
        cfg.ansi.red = "invalid".to_string();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn deserialize_full() {
        let toml_str = r##"
            background = "#000000"
            foreground = "#ffffff"

            [ansi]
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
        "##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.background, "#000000");
        assert_eq!(cfg.ansi.red, "#ff0000");
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn deserialize_partial() {
        let toml_str = r##"
            background = "#000000"
        "##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.background, "#000000");
        // foreground should default
        assert_eq!(cfg.foreground, "#cdd6f4");
    }

    #[test]
    fn deserialize_empty() {
        let cfg: ThemeConfig = toml::from_str("").unwrap();
        assert_eq!(cfg, ThemeConfig::default());
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = ThemeConfig::default();
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: ThemeConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }
}
