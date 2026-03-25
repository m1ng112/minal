//! Theme and color configuration.
//!
//! Default palette is Catppuccin Mocha. Built-in presets are available
//! for Tokyo Night, Dracula, Solarized Dark, and Solarized Light.

use serde::{Deserialize, Serialize};

use crate::ConfigError;

/// Built-in theme presets.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    /// Catppuccin Mocha (default).
    #[default]
    CatppuccinMocha,
    /// Tokyo Night.
    TokyoNight,
    /// Dracula.
    Dracula,
    /// Solarized Dark.
    Solarized,
    /// Solarized Light.
    SolarizedLight,
    /// High Contrast (accessibility).
    HighContrast,
    /// User-defined custom colors.
    Custom,
}

/// Returns the full theme configuration for a built-in preset.
///
/// For `ThemePreset::Custom` this returns the Catppuccin Mocha defaults,
/// since custom themes are defined entirely by per-field overrides.
pub fn builtin_theme(preset: ThemePreset) -> ThemeConfig {
    match preset {
        ThemePreset::CatppuccinMocha | ThemePreset::Custom => ThemeConfig::default(),
        ThemePreset::TokyoNight => ThemeConfig {
            theme: ThemePreset::TokyoNight,
            background: "#1a1b26".to_string(),
            foreground: "#c0caf5".to_string(),
            ansi: AnsiColors {
                black: "#15161e".to_string(),
                red: "#f7768e".to_string(),
                green: "#9ece6a".to_string(),
                yellow: "#e0af68".to_string(),
                blue: "#7aa2f7".to_string(),
                magenta: "#bb9af7".to_string(),
                cyan: "#7dcfff".to_string(),
                white: "#a9b1d6".to_string(),
                bright_black: "#414868".to_string(),
                bright_red: "#f7768e".to_string(),
                bright_green: "#9ece6a".to_string(),
                bright_yellow: "#e0af68".to_string(),
                bright_blue: "#7aa2f7".to_string(),
                bright_magenta: "#bb9af7".to_string(),
                bright_cyan: "#7dcfff".to_string(),
                bright_white: "#c0caf5".to_string(),
            },
        },
        ThemePreset::Dracula => ThemeConfig {
            theme: ThemePreset::Dracula,
            background: "#282a36".to_string(),
            foreground: "#f8f8f2".to_string(),
            ansi: AnsiColors {
                black: "#21222c".to_string(),
                red: "#ff5555".to_string(),
                green: "#50fa7b".to_string(),
                yellow: "#f1fa8c".to_string(),
                blue: "#bd93f9".to_string(),
                magenta: "#ff79c6".to_string(),
                cyan: "#8be9fd".to_string(),
                white: "#f8f8f2".to_string(),
                bright_black: "#6272a4".to_string(),
                bright_red: "#ff6e6e".to_string(),
                bright_green: "#69ff94".to_string(),
                bright_yellow: "#ffffa5".to_string(),
                bright_blue: "#d6acff".to_string(),
                bright_magenta: "#ff92df".to_string(),
                bright_cyan: "#a4ffff".to_string(),
                bright_white: "#ffffff".to_string(),
            },
        },
        ThemePreset::Solarized => ThemeConfig {
            theme: ThemePreset::Solarized,
            background: "#002b36".to_string(),
            foreground: "#839496".to_string(),
            ansi: AnsiColors {
                black: "#073642".to_string(),
                red: "#dc322f".to_string(),
                green: "#859900".to_string(),
                yellow: "#b58900".to_string(),
                blue: "#268bd2".to_string(),
                magenta: "#d33682".to_string(),
                cyan: "#2aa198".to_string(),
                white: "#eee8d5".to_string(),
                bright_black: "#002b36".to_string(),
                bright_red: "#cb4b16".to_string(),
                bright_green: "#586e75".to_string(),
                bright_yellow: "#657b83".to_string(),
                bright_blue: "#839496".to_string(),
                bright_magenta: "#6c71c4".to_string(),
                bright_cyan: "#93a1a1".to_string(),
                bright_white: "#fdf6e3".to_string(),
            },
        },
        ThemePreset::SolarizedLight => ThemeConfig {
            theme: ThemePreset::SolarizedLight,
            background: "#fdf6e3".to_string(),
            foreground: "#657b83".to_string(),
            ansi: AnsiColors {
                black: "#073642".to_string(),
                red: "#dc322f".to_string(),
                green: "#859900".to_string(),
                yellow: "#b58900".to_string(),
                blue: "#268bd2".to_string(),
                magenta: "#d33682".to_string(),
                cyan: "#2aa198".to_string(),
                white: "#eee8d5".to_string(),
                bright_black: "#002b36".to_string(),
                bright_red: "#cb4b16".to_string(),
                bright_green: "#586e75".to_string(),
                bright_yellow: "#657b83".to_string(),
                bright_blue: "#839496".to_string(),
                bright_magenta: "#6c71c4".to_string(),
                bright_cyan: "#93a1a1".to_string(),
                bright_white: "#fdf6e3".to_string(),
            },
        },
        ThemePreset::HighContrast => ThemeConfig {
            theme: ThemePreset::HighContrast,
            background: "#000000".to_string(),
            foreground: "#ffffff".to_string(),
            ansi: AnsiColors {
                black: "#000000".to_string(),
                red: "#ff0000".to_string(),
                green: "#00ff00".to_string(),
                yellow: "#ffff00".to_string(),
                blue: "#0000ff".to_string(),
                magenta: "#ff00ff".to_string(),
                cyan: "#00ffff".to_string(),
                white: "#ffffff".to_string(),
                bright_black: "#808080".to_string(),
                bright_red: "#ff0000".to_string(),
                bright_green: "#00ff00".to_string(),
                bright_yellow: "#ffff00".to_string(),
                bright_blue: "#5c5cff".to_string(),
                bright_magenta: "#ff00ff".to_string(),
                bright_cyan: "#00ffff".to_string(),
                bright_white: "#ffffff".to_string(),
            },
        },
    }
}

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
    /// Selected theme preset. When set to a non-Custom value, the preset
    /// colors override individual color fields.
    #[serde(default)]
    pub theme: ThemePreset,
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
            theme: ThemePreset::default(),
            background: "#1e1e2e".to_string(),
            foreground: "#cdd6f4".to_string(),
            ansi: AnsiColors::default(),
        }
    }
}

impl ThemeConfig {
    /// Resolves the theme configuration by applying the selected preset.
    ///
    /// When `theme` is set to an explicit preset (Tokyo Night, Dracula,
    /// Solarized, Solarized Light), returns the full built-in theme for that
    /// preset. When `theme` is `Custom` or `CatppuccinMocha` (the default),
    /// returns `self` unchanged so that individual per-field color overrides
    /// are preserved.
    pub fn resolve(&self) -> ThemeConfig {
        match self.theme {
            ThemePreset::CatppuccinMocha | ThemePreset::Custom => self.clone(),
            preset => builtin_theme(preset),
        }
    }

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

    #[test]
    fn builtin_theme_tokyo_night() {
        let theme = builtin_theme(ThemePreset::TokyoNight);
        assert_eq!(theme.background, "#1a1b26");
        assert_eq!(theme.foreground, "#c0caf5");
        assert_eq!(theme.ansi.red, "#f7768e");
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn builtin_theme_dracula() {
        let theme = builtin_theme(ThemePreset::Dracula);
        assert_eq!(theme.background, "#282a36");
        assert_eq!(theme.foreground, "#f8f8f2");
        assert_eq!(theme.ansi.green, "#50fa7b");
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn builtin_theme_solarized() {
        let theme = builtin_theme(ThemePreset::Solarized);
        assert_eq!(theme.background, "#002b36");
        assert_eq!(theme.foreground, "#839496");
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn builtin_theme_solarized_light() {
        let theme = builtin_theme(ThemePreset::SolarizedLight);
        assert_eq!(theme.background, "#fdf6e3");
        assert_eq!(theme.foreground, "#657b83");
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn builtin_theme_custom_returns_default() {
        let theme = builtin_theme(ThemePreset::Custom);
        assert_eq!(theme, ThemeConfig::default());
    }

    #[test]
    fn builtin_theme_high_contrast() {
        let theme = builtin_theme(ThemePreset::HighContrast);
        assert_eq!(theme.background, "#000000");
        assert_eq!(theme.foreground, "#ffffff");
        assert_eq!(theme.ansi.red, "#ff0000");
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn all_builtin_themes_are_valid() {
        let presets = [
            ThemePreset::CatppuccinMocha,
            ThemePreset::TokyoNight,
            ThemePreset::Dracula,
            ThemePreset::Solarized,
            ThemePreset::SolarizedLight,
            ThemePreset::HighContrast,
            ThemePreset::Custom,
        ];
        for preset in &presets {
            let theme = builtin_theme(*preset);
            assert!(theme.validate().is_ok(), "preset {preset:?} is invalid");
        }
    }

    #[test]
    fn resolve_with_preset_returns_preset() {
        let cfg = ThemeConfig {
            theme: ThemePreset::Dracula,
            ..ThemeConfig::default()
        };
        let resolved = cfg.resolve();
        assert_eq!(resolved.background, "#282a36");
        assert_eq!(resolved.foreground, "#f8f8f2");
    }

    #[test]
    fn resolve_with_custom_returns_self() {
        let cfg = ThemeConfig {
            theme: ThemePreset::Custom,
            background: "#123456".to_string(),
            foreground: "#abcdef".to_string(),
            ansi: AnsiColors::default(),
        };
        let resolved = cfg.resolve();
        assert_eq!(resolved.background, "#123456");
        assert_eq!(resolved.foreground, "#abcdef");
    }

    #[test]
    fn deserialize_theme_preset_field() {
        let toml_str = r##"
            theme = "tokyo-night"
            background = "#000000"
            foreground = "#ffffff"
        "##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.theme, ThemePreset::TokyoNight);
    }

    #[test]
    fn deserialize_theme_preset_dracula() {
        let toml_str = r##"
            theme = "dracula"
        "##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.theme, ThemePreset::Dracula);
    }

    #[test]
    fn deserialize_theme_preset_high_contrast() {
        let toml_str = r##"
            theme = "high-contrast"
        "##;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.theme, ThemePreset::HighContrast);
    }

    #[test]
    fn resolve_with_high_contrast_returns_preset() {
        let cfg = ThemeConfig {
            theme: ThemePreset::HighContrast,
            ..ThemeConfig::default()
        };
        let resolved = cfg.resolve();
        assert_eq!(resolved.background, "#000000");
        assert_eq!(resolved.foreground, "#ffffff");
    }
}
