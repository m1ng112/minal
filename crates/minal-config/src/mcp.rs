//! MCP (Model Context Protocol) server configuration.
//!
//! Defines server entries for `mcp_servers.toml` and provides
//! loading/validation for MCP server definitions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// MCP transport type.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// Communicate via child process stdin/stdout (default).
    #[default]
    Stdio,
    /// Communicate via HTTP Server-Sent Events.
    Sse,
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct McpServerConfig {
    /// Transport type.
    pub transport: McpTransport,
    /// Command to launch the server (stdio transport).
    pub command: Option<String>,
    /// Arguments for the server command.
    pub args: Vec<String>,
    /// URL for the SSE endpoint (SSE transport).
    pub url: Option<String>,
    /// Environment variables to set for the server process.
    pub env: BTreeMap<String, String>,
    /// Whether this server is enabled.
    pub enabled: bool,
    /// Whether to auto-start this server when Minal launches.
    pub auto_start: bool,
    /// Timeout for tool calls in seconds.
    #[serde(default = "default_tool_timeout_secs")]
    pub tool_timeout_secs: u64,
}

fn default_tool_timeout_secs() -> u64 {
    30
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            transport: McpTransport::default(),
            command: None,
            args: Vec::new(),
            url: None,
            env: BTreeMap::new(),
            enabled: true,
            auto_start: true,
            tool_timeout_secs: default_tool_timeout_secs(),
        }
    }
}

impl McpServerConfig {
    /// Validates the server configuration.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if the configuration is invalid.
    pub fn validate(&self, name: &str) -> Result<(), super::ConfigError> {
        match self.transport {
            McpTransport::Stdio => {
                if self.command.is_none() || self.command.as_deref() == Some("") {
                    return Err(super::ConfigError::Validation(format!(
                        "MCP server '{name}': stdio transport requires a 'command'"
                    )));
                }
            }
            McpTransport::Sse => {
                if self.url.is_none() || self.url.as_deref() == Some("") {
                    return Err(super::ConfigError::Validation(format!(
                        "MCP server '{name}': SSE transport requires a 'url'"
                    )));
                }
            }
        }
        if self.tool_timeout_secs == 0 || self.tool_timeout_secs > 600 {
            return Err(super::ConfigError::Validation(format!(
                "MCP server '{name}': tool_timeout_secs must be between 1 and 600, got {}",
                self.tool_timeout_secs
            )));
        }
        Ok(())
    }
}

/// MCP configuration containing all server definitions.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct McpConfig {
    /// Whether MCP is globally enabled.
    pub enabled: bool,
    /// Map of server name to configuration.
    pub servers: BTreeMap<String, McpServerConfig>,
}

impl McpConfig {
    /// Returns the path to `mcp_servers.toml`.
    ///
    /// # Errors
    /// Returns `ConfigError::ConfigDir` if the config directory cannot be determined.
    pub fn config_path() -> Result<PathBuf, super::ConfigError> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            tracing::error!("could not determine config directory");
            super::ConfigError::ConfigDir
        })?;
        Ok(config_dir.join("minal").join("mcp_servers.toml"))
    }

    /// Loads MCP configuration from the default path.
    ///
    /// # Errors
    /// Returns `ConfigError` on I/O errors (other than not-found) or parse errors.
    pub fn load() -> Result<Self, super::ConfigError> {
        let path = Self::config_path()?;
        Self::load_from(&path)
    }

    /// Loads MCP configuration from a specific path.
    ///
    /// If the file does not exist, returns default configuration.
    ///
    /// # Errors
    /// Returns `ConfigError` on I/O errors (other than not-found) or parse errors.
    pub fn load_from(path: &Path) -> Result<Self, super::ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let config: Self = toml::from_str(&contents)?;
                config.validate()?;
                tracing::info!(
                    ?path,
                    servers = config.servers.len(),
                    "loaded MCP configuration"
                );
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(?path, "MCP config file not found, using defaults");
                Ok(Self::default())
            }
            Err(e) => Err(super::ConfigError::Io(e)),
        }
    }

    /// Validates all server configurations.
    ///
    /// # Errors
    /// Returns `ConfigError::Validation` if any server configuration is invalid.
    pub fn validate(&self) -> Result<(), super::ConfigError> {
        for (name, server) in &self.servers {
            server.validate(name)?;
        }
        Ok(())
    }

    /// Returns an iterator of enabled, auto-start servers.
    pub fn auto_start_servers(&self) -> impl Iterator<Item = (&str, &McpServerConfig)> {
        self.servers
            .iter()
            .filter(|(_, s)| s.enabled && s.auto_start)
            .map(|(name, config)| (name.as_str(), config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = McpConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.servers.is_empty());
    }

    #[test]
    fn deserialize_stdio_server() {
        let toml_str = r#"
            enabled = true
            [servers.filesystem]
            transport = "stdio"
            command = "npx"
            args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
        "#;
        let cfg: McpConfig = toml::from_str(toml_str).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.servers.len(), 1);
        let fs = &cfg.servers["filesystem"];
        assert_eq!(fs.transport, McpTransport::Stdio);
        assert_eq!(fs.command, Some("npx".to_string()));
        assert!(fs.enabled);
        assert!(fs.auto_start);
    }

    #[test]
    fn deserialize_sse_server() {
        let toml_str = r#"
            enabled = true
            [servers.remote]
            transport = "sse"
            url = "http://localhost:3000/sse"
            auto_start = false
        "#;
        let cfg: McpConfig = toml::from_str(toml_str).unwrap();
        let remote = &cfg.servers["remote"];
        assert_eq!(remote.transport, McpTransport::Sse);
        assert_eq!(remote.url, Some("http://localhost:3000/sse".to_string()));
        assert!(!remote.auto_start);
    }

    #[test]
    fn deserialize_with_env() {
        let toml_str = r#"
            enabled = true
            [servers.myserver]
            command = "my-mcp-server"
            [servers.myserver.env]
            API_KEY = "test-key"
            DEBUG = "1"
        "#;
        let cfg: McpConfig = toml::from_str(toml_str).unwrap();
        let server = &cfg.servers["myserver"];
        assert_eq!(server.env.get("API_KEY"), Some(&"test-key".to_string()));
        assert_eq!(server.env.get("DEBUG"), Some(&"1".to_string()));
    }

    #[test]
    fn validate_stdio_without_command() {
        let cfg = McpConfig {
            enabled: true,
            servers: {
                let mut m = BTreeMap::new();
                m.insert(
                    "bad".to_string(),
                    McpServerConfig {
                        transport: McpTransport::Stdio,
                        command: None,
                        ..McpServerConfig::default()
                    },
                );
                m
            },
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_sse_without_url() {
        let cfg = McpConfig {
            enabled: true,
            servers: {
                let mut m = BTreeMap::new();
                m.insert(
                    "bad".to_string(),
                    McpServerConfig {
                        transport: McpTransport::Sse,
                        url: None,
                        ..McpServerConfig::default()
                    },
                );
                m
            },
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_timeout_zero() {
        let mut cfg = McpServerConfig::default();
        cfg.command = Some("test".to_string());
        cfg.tool_timeout_secs = 0;
        assert!(cfg.validate("test").is_err());
    }

    #[test]
    fn validate_valid_stdio() {
        let cfg = McpConfig {
            enabled: true,
            servers: {
                let mut m = BTreeMap::new();
                m.insert(
                    "good".to_string(),
                    McpServerConfig {
                        transport: McpTransport::Stdio,
                        command: Some("my-server".to_string()),
                        ..McpServerConfig::default()
                    },
                );
                m
            },
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = McpConfig {
            enabled: true,
            servers: {
                let mut m = BTreeMap::new();
                m.insert(
                    "test".to_string(),
                    McpServerConfig {
                        transport: McpTransport::Stdio,
                        command: Some("test-cmd".to_string()),
                        args: vec!["--flag".to_string()],
                        ..McpServerConfig::default()
                    },
                );
                m
            },
        };
        let s = toml::to_string(&cfg).unwrap();
        let cfg2: McpConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, cfg2);
    }

    #[test]
    fn deserialize_empty() {
        let cfg: McpConfig = toml::from_str("").unwrap();
        assert_eq!(cfg, McpConfig::default());
    }

    #[test]
    fn load_from_nonexistent_returns_defaults() {
        let path = std::path::Path::new("/tmp/minal_test_nonexistent_mcp.toml");
        let cfg = McpConfig::load_from(path).unwrap();
        assert_eq!(cfg, McpConfig::default());
    }

    #[test]
    fn auto_start_servers_filter() {
        let cfg = McpConfig {
            enabled: true,
            servers: {
                let mut m = BTreeMap::new();
                m.insert(
                    "auto".to_string(),
                    McpServerConfig {
                        command: Some("auto-cmd".to_string()),
                        enabled: true,
                        auto_start: true,
                        ..McpServerConfig::default()
                    },
                );
                m.insert(
                    "manual".to_string(),
                    McpServerConfig {
                        command: Some("manual-cmd".to_string()),
                        enabled: true,
                        auto_start: false,
                        ..McpServerConfig::default()
                    },
                );
                m.insert(
                    "disabled".to_string(),
                    McpServerConfig {
                        command: Some("disabled-cmd".to_string()),
                        enabled: false,
                        auto_start: true,
                        ..McpServerConfig::default()
                    },
                );
                m
            },
        };
        let auto: Vec<_> = cfg.auto_start_servers().collect();
        assert_eq!(auto.len(), 1);
        assert_eq!(auto[0].0, "auto");
    }

    #[test]
    fn config_path_valid() {
        let path = McpConfig::config_path().unwrap();
        assert!(path.ends_with("minal/mcp_servers.toml"));
    }
}
