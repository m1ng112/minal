//! Font configuration.

use serde::{Deserialize, Serialize};

use crate::ConfigError;

/// Font settings for the terminal.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct FontConfig {
    /// Font family name.
    pub family: String,
    /// Font size in points.
    pub size: f32,
    /// Line height multiplier. If `None`, defaults to `size * 1.2`.
    pub line_height: Option<f32>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "JetBrains Mono".to_string(),
            size: 14.0,
            line_height: None,
        }
    }
}

impl FontConfig {
    /// Validates the font configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any value is out of range.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.size <= 0.0 || self.size > 200.0 {
            return Err(ConfigError::Validation(format!(
                "font.size must be > 0 and <= 200, got {}",
                self.size
            )));
        }
        if let Some(lh) = self.line_height {
            if lh <= 0.0 {
                return Err(ConfigError::Validation(format!(
                    "font.line_height must be > 0 if set, got {lh}"
                )));
            }
        }
        Ok(())
    }

    /// Returns the effective line height (explicit value or `size * 1.2`).
    pub fn effective_line_height(&self) -> f32 {
        self.line_height.unwrap_or(self.size * 1.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let cfg = FontConfig::default();
        assert_eq!(cfg.family, "JetBrains Mono");
        assert!((cfg.size - 14.0).abs() < f32::EPSILON);
        assert_eq!(cfg.line_height, None);
    }

    #[test]
    fn effective_line_height_auto() {
        let cfg = FontConfig::default();
        assert!((cfg.effective_line_height() - 16.8).abs() < 0.01);
    }

    #[test]
    fn effective_line_height_explicit() {
        let cfg = FontConfig {
            line_height: Some(20.0),
            ..FontConfig::default()
        };
        assert!((cfg.effective_line_height() - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn validate_valid() {
        let cfg = FontConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_zero_size() {
        let cfg = FontConfig {
            size: 0.0,
            ..FontConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_negative_size() {
        let cfg = FontConfig {
            size: -1.0,
            ..FontConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_too_large_size() {
        let cfg = FontConfig {
            size: 201.0,
            ..FontConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_negative_line_height() {
        let cfg = FontConfig {
            line_height: Some(-1.0),
            ..FontConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_zero_line_height() {
        let cfg = FontConfig {
            line_height: Some(0.0),
            ..FontConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn deserialize_full() {
        let toml_str = r#"
            family = "Fira Code"
            size = 18.0
            line_height = 22.0
        "#;
        let cfg: FontConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.family, "Fira Code");
        assert!((cfg.size - 18.0).abs() < f32::EPSILON);
        assert_eq!(cfg.line_height, Some(22.0));
    }

    #[test]
    fn deserialize_partial() {
        let toml_str = r#"
            size = 20.0
        "#;
        let cfg: FontConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.family, "JetBrains Mono");
        assert!((cfg.size - 20.0).abs() < f32::EPSILON);
        assert_eq!(cfg.line_height, None);
    }

    #[test]
    fn deserialize_empty() {
        let cfg: FontConfig = toml::from_str("").unwrap();
        assert_eq!(cfg, FontConfig::default());
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = FontConfig::default();
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: FontConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }
}
