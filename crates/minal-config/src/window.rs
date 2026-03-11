//! Window configuration.

use serde::Deserialize;

/// Window settings for the terminal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Number of columns.
    pub columns: u16,
    /// Number of rows.
    pub rows: u16,
    /// Window title.
    pub title: String,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            columns: 80,
            rows: 24,
            title: "Minal".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let win = WindowConfig::default();
        assert_eq!(win.columns, 80);
        assert_eq!(win.rows, 24);
    }

    #[test]
    fn test_custom() {
        let toml_str = r#"columns = 120
rows = 40"#;
        let win: WindowConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(win.columns, 120);
        assert_eq!(win.rows, 40);
    }
}
