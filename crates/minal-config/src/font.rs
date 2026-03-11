//! Font configuration.

use serde::Deserialize;

/// Font settings for the terminal.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    /// Font family name.
    pub family: String,
    /// Font size in points.
    pub size: f32,
    /// Line height multiplier.
    pub line_height: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "JetBrains Mono".to_string(),
            size: 14.0,
            line_height: 1.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let font = FontConfig::default();
        assert_eq!(font.family, "JetBrains Mono");
        assert_eq!(font.size, 14.0);
    }

    #[test]
    fn test_partial_override() {
        let toml_str = r#"size = 16.0"#;
        let font: FontConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(font.family, "JetBrains Mono");
        assert_eq!(font.size, 16.0);
    }
}
