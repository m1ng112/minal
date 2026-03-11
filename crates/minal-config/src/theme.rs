//! Theme / color configuration.
//!
//! Defaults to Catppuccin Mocha.

use crate::error::ConfigError;
use serde::Deserialize;

/// Color theme settings for the terminal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Background color.
    pub background: String,
    /// Foreground color.
    pub foreground: String,
    /// Cursor color.
    pub cursor: String,

    // Standard ANSI colors (0-7).
    /// ANSI black.
    pub black: String,
    /// ANSI red.
    pub red: String,
    /// ANSI green.
    pub green: String,
    /// ANSI yellow.
    pub yellow: String,
    /// ANSI blue.
    pub blue: String,
    /// ANSI magenta.
    pub magenta: String,
    /// ANSI cyan.
    pub cyan: String,
    /// ANSI white.
    pub white: String,

    // Bright ANSI colors (8-15).
    /// Bright black.
    pub bright_black: String,
    /// Bright red.
    pub bright_red: String,
    /// Bright green.
    pub bright_green: String,
    /// Bright yellow.
    pub bright_yellow: String,
    /// Bright blue.
    pub bright_blue: String,
    /// Bright magenta.
    pub bright_magenta: String,
    /// Bright cyan.
    pub bright_cyan: String,
    /// Bright white.
    pub bright_white: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            background: "#1e1e2e".to_string(),
            foreground: "#cdd6f4".to_string(),
            cursor: "#f5e0dc".to_string(),
            black: "#2e3040".to_string(),
            red: "#f38ba8".to_string(),
            green: "#a6e3a1".to_string(),
            yellow: "#f9e2af".to_string(),
            blue: "#89b4fa".to_string(),
            magenta: "#cba6f7".to_string(),
            cyan: "#94e2d5".to_string(),
            white: "#bac2de".to_string(),
            bright_black: "#6c7086".to_string(),
            bright_red: "#f38ba8".to_string(),
            bright_green: "#a6e3a1".to_string(),
            bright_yellow: "#f9e2af".to_string(),
            bright_blue: "#89b4fa".to_string(),
            bright_magenta: "#cba6f7".to_string(),
            bright_cyan: "#94e2d5".to_string(),
            bright_white: "#cdd6f4".to_string(),
        }
    }
}

impl ThemeConfig {
    /// Parse a hex color string like `#RRGGBB` into `(r, g, b)`.
    pub fn parse_hex_color(hex: &str) -> Result<(u8, u8, u8), ConfigError> {
        let hex = hex.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 {
            return Err(ConfigError::InvalidColor(format!(
                "expected 6 hex digits, got: {hex}"
            )));
        }
        let r = u8::from_str_radix(&hex[0..2], 16)
            .map_err(|_| ConfigError::InvalidColor(format!("invalid red component: {hex}")))?;
        let g = u8::from_str_radix(&hex[2..4], 16)
            .map_err(|_| ConfigError::InvalidColor(format!("invalid green component: {hex}")))?;
        let b = u8::from_str_radix(&hex[4..6], 16)
            .map_err(|_| ConfigError::InvalidColor(format!("invalid blue component: {hex}")))?;
        Ok((r, g, b))
    }

    /// Get a named ANSI color by index (0-15) as `(r, g, b)`.
    pub fn ansi_color(&self, index: u8) -> Result<(u8, u8, u8), ConfigError> {
        let hex = match index {
            0 => &self.black,
            1 => &self.red,
            2 => &self.green,
            3 => &self.yellow,
            4 => &self.blue,
            5 => &self.magenta,
            6 => &self.cyan,
            7 => &self.white,
            8 => &self.bright_black,
            9 => &self.bright_red,
            10 => &self.bright_green,
            11 => &self.bright_yellow,
            12 => &self.bright_blue,
            13 => &self.bright_magenta,
            14 => &self.bright_cyan,
            15 => &self.bright_white,
            _ => {
                return Err(ConfigError::InvalidColor(format!(
                    "invalid ANSI color index: {index}"
                )));
            }
        };
        Self::parse_hex_color(hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_valid() {
        assert_eq!(
            ThemeConfig::parse_hex_color("#ff0000").unwrap(),
            (255, 0, 0)
        );
        assert_eq!(
            ThemeConfig::parse_hex_color("#1e1e2e").unwrap(),
            (30, 30, 46)
        );
    }

    #[test]
    fn test_parse_hex_no_hash() {
        assert_eq!(ThemeConfig::parse_hex_color("ff0000").unwrap(), (255, 0, 0));
    }

    #[test]
    fn test_parse_hex_invalid_length() {
        assert!(ThemeConfig::parse_hex_color("#fff").is_err());
    }

    #[test]
    fn test_parse_hex_invalid_chars() {
        assert!(ThemeConfig::parse_hex_color("#gggggg").is_err());
    }

    #[test]
    fn test_default_catppuccin() {
        let theme = ThemeConfig::default();
        assert_eq!(theme.background, "#1e1e2e");
        assert_eq!(theme.foreground, "#cdd6f4");
    }

    #[test]
    fn test_ansi_color() {
        let theme = ThemeConfig::default();
        let (r, g, b) = theme.ansi_color(1).unwrap();
        assert_eq!((r, g, b), (243, 139, 168)); // #f38ba8
    }
}
