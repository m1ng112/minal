//! Plugin manager — loads, initializes, and dispatches events to plugins.
//!
//! The [`PluginManager`] scans configured plugin directories, loads valid
//! plugins, manages their lifecycle, and routes events to subscribed hooks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ai_bridge::WasmAiProvider;
use crate::error::PluginError;
use crate::event::{HookRegistry, HookResponse, PluginEvent};
use crate::manifest::PluginManifest;
use crate::runtime::{PluginInstance, WasiRuntime};

/// Metadata and state of a loaded plugin.
struct LoadedPlugin {
    /// Parsed manifest.
    manifest: PluginManifest,
    /// Directory containing the plugin.
    dir: PathBuf,
    /// WASM instance (None if the plugin was unloaded or is an AI-only plugin
    /// whose instance was moved to a [`WasmAiProvider`]).
    instance: Option<PluginInstance>,
}

/// Manages the lifecycle of all loaded plugins.
pub struct PluginManager {
    /// Shared WASI runtime engine.
    runtime: WasiRuntime,
    /// Loaded plugins indexed by position.
    /// Entries are never removed; unloaded plugins have `instance: None`.
    plugins: Vec<LoadedPlugin>,
    /// Name → index lookup.
    name_index: HashMap<String, usize>,
    /// Hook dispatch registry.
    hooks: HookRegistry,
    /// If non-empty, only these plugin names are allowed to load.
    allowed_plugins: Vec<String>,
}

impl PluginManager {
    /// Create a new plugin manager.
    ///
    /// If `allowed_plugins` is non-empty, only plugins with names in the list
    /// will be loaded; all others are rejected.
    ///
    /// # Errors
    /// Returns `PluginError::Runtime` if the WASI engine cannot be initialized.
    pub fn new(allowed_plugins: Vec<String>) -> Result<Self, PluginError> {
        let runtime = WasiRuntime::new()?;
        Ok(Self {
            runtime,
            plugins: Vec::new(),
            name_index: HashMap::new(),
            hooks: HookRegistry::new(),
            allowed_plugins,
        })
    }

    /// Scan a directory for plugin subdirectories and load each one.
    ///
    /// Each subdirectory should contain a `plugin.toml` manifest and a `.wasm`
    /// file. Plugins that fail to load are logged and skipped.
    ///
    /// # Errors
    /// Returns `PluginError::DirNotFound` if `plugin_dir` does not exist.
    pub fn scan_directory(&mut self, plugin_dir: &Path) -> Result<Vec<String>, PluginError> {
        if !plugin_dir.is_dir() {
            return Err(PluginError::DirNotFound(plugin_dir.display().to_string()));
        }

        let mut loaded_names = Vec::new();
        let entries = std::fs::read_dir(plugin_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("plugin.toml");
            if !manifest_path.exists() {
                tracing::debug!(dir = %path.display(), "skipping directory without plugin.toml");
                continue;
            }

            match self.load_plugin(&path) {
                Ok(name) => {
                    tracing::info!(plugin = %name, "loaded plugin");
                    loaded_names.push(name);
                }
                Err(e) => {
                    tracing::warn!(
                        dir = %path.display(),
                        error = %e,
                        "failed to load plugin, skipping"
                    );
                }
            }
        }

        Ok(loaded_names)
    }

    /// Load a single plugin from its directory.
    ///
    /// The directory must contain a `plugin.toml` manifest.
    ///
    /// # Errors
    /// Returns `PluginError` if the manifest is invalid, the WASM module
    /// cannot be compiled, or the plugin fails to initialize.
    pub fn load_plugin(&mut self, plugin_dir: &Path) -> Result<String, PluginError> {
        let manifest_path = plugin_dir.join("plugin.toml");
        let manifest = PluginManifest::load(&manifest_path)?;
        let name = manifest.plugin.name.clone();

        // Enforce allowlist.
        if !self.allowed_plugins.is_empty() && !self.allowed_plugins.contains(&name) {
            return Err(PluginError::ManifestValidation(format!(
                "plugin '{name}' is not in the allowed_plugins list"
            )));
        }

        // Prevent duplicate loading.
        if self.name_index.contains_key(&name) {
            return Err(PluginError::ManifestValidation(format!(
                "plugin '{name}' is already loaded"
            )));
        }

        let wasm_path = manifest.resolve_wasm_path(plugin_dir);
        if !wasm_path.exists() {
            return Err(PluginError::NotFound(format!(
                "WASM module not found: {}",
                wasm_path.display()
            )));
        }

        let mut instance = self.runtime.load_plugin(&wasm_path, plugin_dir)?;
        instance.call_init()?;

        let index = self.plugins.len();

        // Register hooks.
        self.hooks.register(
            index,
            manifest.hooks.on_command,
            manifest.hooks.on_output,
            manifest.hooks.on_error,
        );

        self.name_index.insert(name.clone(), index);
        self.plugins.push(LoadedPlugin {
            manifest,
            dir: plugin_dir.to_path_buf(),
            instance: Some(instance),
        });

        Ok(name)
    }

    /// Unload a plugin by name.
    ///
    /// The plugin's hooks are unregistered and its WASM instance is dropped.
    ///
    /// # Errors
    /// Returns `PluginError::NotFound` if no plugin with that name is loaded.
    pub fn unload_plugin(&mut self, name: &str) -> Result<(), PluginError> {
        let &index = self
            .name_index
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        self.hooks.unregister(index);
        self.plugins[index].instance = None;
        self.name_index.remove(name);

        tracing::info!(plugin = %name, "unloaded plugin");
        Ok(())
    }

    /// Dispatch an event to all plugins subscribed to its hook.
    ///
    /// Returns the aggregated responses. If any plugin sets `suppress = true`,
    /// subsequent plugins still run but the caller should suppress the event.
    ///
    /// # Errors
    /// Returns the first `PluginError` encountered. Subsequent plugins are
    /// still called; errors are logged.
    pub fn dispatch_event(
        &mut self,
        event: &PluginEvent,
    ) -> Result<Vec<HookResponse>, PluginError> {
        let subscriber_indices: Vec<usize> = self.hooks.subscribers(event).to_vec();
        let mut responses = Vec::new();
        let mut first_error: Option<PluginError> = None;

        for &index in &subscriber_indices {
            let plugin = &mut self.plugins[index];
            let Some(ref mut instance) = plugin.instance else {
                continue;
            };

            match instance.dispatch_event(event) {
                Ok(resp) => {
                    if let Some(ref msg) = resp.message {
                        tracing::debug!(
                            plugin = %plugin.manifest.plugin.name,
                            message = %msg,
                            "plugin hook message"
                        );
                    }
                    responses.push(resp);
                }
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.plugin.name,
                        error = %e,
                        "plugin hook dispatch failed"
                    );
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        if let Some(e) = first_error {
            return Err(e);
        }

        Ok(responses)
    }

    /// Take ownership of a plugin's instance for use as an AI provider.
    ///
    /// This removes the instance from the plugin manager (it can no longer
    /// receive hook events) and wraps it in a [`WasmAiProvider`].
    ///
    /// # Errors
    /// Returns `PluginError::NotFound` if the plugin is not loaded, or
    /// `PluginError::NotLoaded` if the instance was already taken.
    pub fn take_ai_provider(&mut self, name: &str) -> Result<WasmAiProvider, PluginError> {
        let &index = self
            .name_index
            .get(name)
            .ok_or_else(|| PluginError::NotFound(name.to_string()))?;

        let plugin = &mut self.plugins[index];
        let ai_config = plugin.manifest.ai_provider.as_ref().ok_or_else(|| {
            PluginError::ManifestValidation(format!(
                "plugin '{name}' does not declare an ai_provider section"
            ))
        })?;

        let instance = plugin
            .instance
            .take()
            .ok_or_else(|| PluginError::NotLoaded(name.to_string()))?;

        let provider_name = ai_config.name.clone();
        WasmAiProvider::new(provider_name, instance)
    }

    /// Returns `true` if any loaded plugin subscribes to output events.
    pub fn has_output_hooks(&self) -> bool {
        !self.hooks.output_hooks.is_empty()
    }

    /// List all loaded plugin names.
    pub fn loaded_plugins(&self) -> Vec<&str> {
        self.name_index.keys().map(String::as_str).collect()
    }

    /// Get the manifest for a loaded plugin.
    pub fn manifest(&self, name: &str) -> Option<&PluginManifest> {
        self.name_index
            .get(name)
            .map(|&idx| &self.plugins[idx].manifest)
    }

    /// Get the directory path for a loaded plugin.
    pub fn plugin_dir(&self, name: &str) -> Option<&Path> {
        self.name_index
            .get(name)
            .map(|&idx| self.plugins[idx].dir.as_path())
    }

    /// Returns the number of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.name_index.len()
    }

    /// Returns `true` if any loaded plugin provides a custom AI provider.
    pub fn has_ai_providers(&self) -> bool {
        self.plugins.iter().any(|p| {
            self.name_index.contains_key(&p.manifest.plugin.name) && p.manifest.is_ai_provider()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_creation_succeeds() {
        let mgr = PluginManager::new(Vec::new());
        assert!(mgr.is_ok());
        assert_eq!(mgr.as_ref().map(|m| m.plugin_count()).unwrap_or(0), 0);
    }

    #[test]
    fn scan_nonexistent_directory_fails() {
        let mut mgr = PluginManager::new(Vec::new()).expect("manager");
        let result = mgr.scan_directory(Path::new("/nonexistent/plugins"));
        assert!(result.is_err());
    }

    #[test]
    fn scan_empty_directory_returns_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut mgr = PluginManager::new(Vec::new()).expect("manager");
        let result = mgr.scan_directory(dir.path());
        assert!(result.is_ok());
        assert!(result.as_ref().map(|v| v.is_empty()).unwrap_or(false));
    }

    #[test]
    fn load_plugin_without_wasm_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plugin_dir = dir.path().join("test-plugin");
        std::fs::create_dir(&plugin_dir).expect("mkdir");
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            "[plugin]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .expect("write manifest");

        let mut mgr = PluginManager::new(Vec::new()).expect("manager");
        let result = mgr.load_plugin(&plugin_dir);
        assert!(result.is_err());
    }

    #[test]
    fn unload_nonexistent_fails() {
        let mut mgr = PluginManager::new(Vec::new()).expect("manager");
        let result = mgr.unload_plugin("nonexistent");
        assert!(result.is_err());
    }
}
