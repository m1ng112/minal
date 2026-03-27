//! Plugin manifest definition.
//!
//! Each plugin is a directory containing a `plugin.toml` manifest and a
//! `.wasm` module.  The manifest describes the plugin metadata, which event
//! hooks it subscribes to, and whether it provides a custom AI provider.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

/// Top-level plugin manifest parsed from `plugin.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginManifest {
    /// Core plugin metadata.
    pub plugin: PluginMeta,
    /// Event hooks this plugin subscribes to.
    #[serde(default)]
    pub hooks: HookConfig,
    /// Optional AI provider configuration.
    #[serde(default)]
    pub ai_provider: Option<AiProviderConfig>,
}

/// Core plugin metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginMeta {
    /// Unique plugin name (used as identifier).
    pub name: String,
    /// SemVer version string.
    pub version: String,
    /// Human-readable description.
    #[serde(default)]
    pub description: String,
    /// Author name or email.
    #[serde(default)]
    pub author: String,
    /// Relative path to the WASM module within the plugin directory.
    #[serde(default = "default_wasm_path")]
    pub wasm_path: String,
}

fn default_wasm_path() -> String {
    "plugin.wasm".to_string()
}

/// Which event hooks the plugin subscribes to.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct HookConfig {
    /// Called when a shell command is about to execute.
    pub on_command: bool,
    /// Called when terminal output is received.
    pub on_output: bool,
    /// Called when a command exits with a non-zero status.
    pub on_error: bool,
}

/// Configuration for plugins that provide a custom AI backend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiProviderConfig {
    /// Display name for this AI provider.
    pub name: String,
}

impl PluginManifest {
    /// Load a manifest from a `plugin.toml` file.
    ///
    /// # Errors
    /// Returns `PluginError::Io` if the file cannot be read, or
    /// `PluginError::ManifestParse` if the TOML is invalid.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        let contents = std::fs::read_to_string(path)?;
        let manifest: Self = toml::from_str(&contents)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Parse a manifest from a TOML string.
    ///
    /// # Errors
    /// Returns `PluginError::ManifestParse` on parse error or
    /// `PluginError::ManifestValidation` if the manifest is invalid.
    pub fn parse(s: &str) -> Result<Self, PluginError> {
        let manifest: Self = toml::from_str(s)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest contents.
    fn validate(&self) -> Result<(), PluginError> {
        if self.plugin.name.is_empty() {
            return Err(PluginError::ManifestValidation(
                "plugin.name must not be empty".to_string(),
            ));
        }
        if self.plugin.version.is_empty() {
            return Err(PluginError::ManifestValidation(
                "plugin.version must not be empty".to_string(),
            ));
        }
        if self.plugin.wasm_path.is_empty() {
            return Err(PluginError::ManifestValidation(
                "plugin.wasm_path must not be empty".to_string(),
            ));
        }
        if self.plugin.wasm_path.contains("..") {
            return Err(PluginError::ManifestValidation(
                "plugin.wasm_path must not contain '..' (path traversal)".to_string(),
            ));
        }
        if self.plugin.wasm_path.starts_with('/') {
            return Err(PluginError::ManifestValidation(
                "plugin.wasm_path must be a relative path".to_string(),
            ));
        }
        Ok(())
    }

    /// Resolve the absolute path to the WASM module, given the plugin directory.
    pub fn resolve_wasm_path(&self, plugin_dir: &Path) -> PathBuf {
        plugin_dir.join(&self.plugin.wasm_path)
    }

    /// Returns `true` if this plugin subscribes to any event hooks.
    pub fn has_hooks(&self) -> bool {
        self.hooks.on_command || self.hooks.on_output || self.hooks.on_error
    }

    /// Returns `true` if this plugin provides a custom AI provider.
    pub fn is_ai_provider(&self) -> bool {
        self.ai_provider.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_manifest() {
        let toml = r#"
            [plugin]
            name = "example"
            version = "0.1.0"
            description = "An example plugin"
            author = "Test Author"
            wasm_path = "example.wasm"

            [hooks]
            on_command = true
            on_output = false
            on_error = true

            [ai_provider]
            name = "custom-llm"
        "#;
        let m = PluginManifest::parse(toml).expect("should parse");
        assert_eq!(m.plugin.name, "example");
        assert_eq!(m.plugin.version, "0.1.0");
        assert!(m.hooks.on_command);
        assert!(!m.hooks.on_output);
        assert!(m.hooks.on_error);
        assert!(m.is_ai_provider());
        assert_eq!(
            m.ai_provider.as_ref().map(|a| a.name.as_str()),
            Some("custom-llm")
        );
    }

    #[test]
    fn parse_minimal_manifest() {
        let toml = r#"
            [plugin]
            name = "minimal"
            version = "1.0.0"
        "#;
        let m = PluginManifest::parse(toml).expect("should parse");
        assert_eq!(m.plugin.name, "minimal");
        assert_eq!(m.plugin.wasm_path, "plugin.wasm");
        assert!(!m.has_hooks());
        assert!(!m.is_ai_provider());
    }

    #[test]
    fn empty_name_is_rejected() {
        let toml = r#"
            [plugin]
            name = ""
            version = "0.1.0"
        "#;
        assert!(PluginManifest::parse(toml).is_err());
    }

    #[test]
    fn empty_version_is_rejected() {
        let toml = r#"
            [plugin]
            name = "test"
            version = ""
        "#;
        assert!(PluginManifest::parse(toml).is_err());
    }

    #[test]
    fn path_traversal_in_wasm_path_is_rejected() {
        let toml = r#"
            [plugin]
            name = "evil"
            version = "0.1.0"
            wasm_path = "../../etc/passwd"
        "#;
        assert!(PluginManifest::parse(toml).is_err());
    }

    #[test]
    fn absolute_wasm_path_is_rejected() {
        let toml = r#"
            [plugin]
            name = "evil"
            version = "0.1.0"
            wasm_path = "/usr/lib/evil.wasm"
        "#;
        assert!(PluginManifest::parse(toml).is_err());
    }

    #[test]
    fn resolve_wasm_path_joins_dir() {
        let toml = r#"
            [plugin]
            name = "test"
            version = "0.1.0"
            wasm_path = "my_plugin.wasm"
        "#;
        let m = PluginManifest::parse(toml).expect("should parse");
        let path = m.resolve_wasm_path(Path::new("/plugins/test"));
        assert_eq!(path, PathBuf::from("/plugins/test/my_plugin.wasm"));
    }
}
