//! Plugin system configuration.

use serde::{Deserialize, Serialize};

/// Plugin system settings.
///
/// Controls whether the plugin system is enabled and where to find plugins.
///
/// ```toml
/// [plugins]
/// enabled = true
/// plugin_dirs = ["~/.config/minal/plugins"]
/// allowed_plugins = []
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct PluginConfig {
    /// Whether the plugin system is enabled.
    pub enabled: bool,
    /// Directories to scan for plugins.
    /// Supports `~` for home directory expansion.
    pub plugin_dirs: Vec<String>,
    /// If non-empty, only these plugin names are allowed to load.
    /// An empty list means all plugins are allowed.
    pub allowed_plugins: Vec<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            plugin_dirs: vec!["~/.config/minal/plugins".to_string()],
            allowed_plugins: Vec::new(),
        }
    }
}
