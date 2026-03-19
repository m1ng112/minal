//! Built-in theme presets for the terminal.
//!
//! Provides curated color schemes: Catppuccin Mocha, Tokyo Night, Dracula, and Solarized Dark.

use serde::{Deserialize, Serialize};

use crate::theme::{AnsiColors, ThemeConfig};

/// Available built-in theme presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    /// Catppuccin Mocha (default).
    CatppuccinMocha,
    /// Tokyo Night color scheme.
    TokyoNight,
    /// Dracula color scheme.
    Dracula,
    /// Solarized Dark color scheme.
    SolarizedDark,
}

/// Returns the `ThemeConfig` for a given preset.
pub fn preset_theme(preset: ThemePreset) -> ThemeConfig {
    match preset {
        ThemePreset::CatppuccinMocha => catppuccin_mocha(),
        ThemePreset::TokyoNight => tokyo_night(),
        ThemePreset::Dracula => dracula(),
        ThemePreset::SolarizedDark => solarized_dark(),
    }
}

/// Catppuccin Mocha theme (the default).
fn catppuccin_mocha() -> ThemeConfig {
    ThemeConfig {
        theme: Some(ThemePreset::CatppuccinMocha),
        background: "#1e1e2e".to_string(),
        foreground: "#cdd6f4".to_string(),
        ansi: AnsiColors {
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
        },
    }
}

/// Tokyo Night theme.
fn tokyo_night() -> ThemeConfig {
    ThemeConfig {
        theme: Some(ThemePreset::TokyoNight),
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
    }
}

/// Dracula theme.
fn dracula() -> ThemeConfig {
    ThemeConfig {
        theme: Some(ThemePreset::Dracula),
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
    }
}

/// Solarized Dark theme.
fn solarized_dark() -> ThemeConfig {
    ThemeConfig {
        theme: Some(ThemePreset::SolarizedDark),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_validate() {
        for preset in [
            ThemePreset::CatppuccinMocha,
            ThemePreset::TokyoNight,
            ThemePreset::Dracula,
            ThemePreset::SolarizedDark,
        ] {
            let theme = preset_theme(preset);
            theme.validate().unwrap_or_else(|e| {
                panic!("preset {preset:?} failed validation: {e}");
            });
        }
    }

    #[test]
    fn catppuccin_matches_default_colors() {
        let preset = preset_theme(ThemePreset::CatppuccinMocha);
        let default = ThemeConfig::default();
        assert_eq!(preset.background, default.background);
        assert_eq!(preset.foreground, default.foreground);
        assert_eq!(preset.ansi, default.ansi);
        // Preset has theme set, default does not.
        assert_eq!(preset.theme, Some(ThemePreset::CatppuccinMocha));
        assert_eq!(default.theme, None);
    }

    #[test]
    fn deserialize_preset_names() {
        let cases = [
            ("\"catppuccin-mocha\"", ThemePreset::CatppuccinMocha),
            ("\"tokyo-night\"", ThemePreset::TokyoNight),
            ("\"dracula\"", ThemePreset::Dracula),
            ("\"solarized-dark\"", ThemePreset::SolarizedDark),
        ];
        for (json, expected) in &cases {
            let parsed: ThemePreset = serde_json::from_str(json).unwrap_or_else(|e| {
                panic!("failed to parse {json}: {e}");
            });
            assert_eq!(parsed, *expected);
        }
    }

    #[test]
    fn serialize_roundtrip() {
        for preset in [
            ThemePreset::CatppuccinMocha,
            ThemePreset::TokyoNight,
            ThemePreset::Dracula,
            ThemePreset::SolarizedDark,
        ] {
            let json = serde_json::to_string(&preset).unwrap();
            let parsed: ThemePreset = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, preset);
        }
    }

    #[test]
    fn theme_config_with_preset_toml() {
        let toml_str = r#"theme = "dracula""#;
        let cfg: ThemeConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.theme, Some(ThemePreset::Dracula));
    }

    #[test]
    fn theme_resolve_with_preset() {
        let cfg = ThemeConfig {
            theme: Some(ThemePreset::Dracula),
            ..ThemeConfig::default()
        };
        let resolved = cfg.resolve();
        assert_eq!(resolved.background, "#282a36");
        assert_eq!(resolved.foreground, "#f8f8f2");
    }

    #[test]
    fn theme_resolve_with_override() {
        let cfg = ThemeConfig {
            theme: Some(ThemePreset::Dracula),
            background: "#000000".to_string(),
            ..ThemeConfig::default()
        };
        let resolved = cfg.resolve();
        // Background should be overridden (differs from Catppuccin default).
        assert_eq!(resolved.background, "#000000");
        // Foreground should come from Dracula preset (matches Catppuccin default).
        assert_eq!(resolved.foreground, "#f8f8f2");
    }

    #[test]
    fn theme_resolve_without_preset() {
        let cfg = ThemeConfig::default();
        let resolved = cfg.resolve();
        assert_eq!(resolved, cfg);
    }
}
