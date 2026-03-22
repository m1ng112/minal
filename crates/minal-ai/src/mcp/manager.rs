//! MCP server lifecycle manager.

use std::collections::HashMap;

use minal_config::McpServerConfig;

use super::client::McpClient;
use super::registry::McpToolRegistry;
use super::transport::{McpTransportTrait, SseTransport, StdioTransport};
use super::types::{McpToolCallResult, McpToolDefinition};
use crate::error::AiError;

/// Manages MCP server lifecycles and tool dispatch.
pub struct McpServerManager {
    clients: HashMap<String, McpClient>,
    registry: McpToolRegistry,
    server_configs: HashMap<String, McpServerConfig>,
}

impl McpServerManager {
    /// Creates a new empty server manager.
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            registry: McpToolRegistry::new(),
            server_configs: HashMap::new(),
        }
    }

    /// Starts an MCP server and returns its available tools.
    ///
    /// If a server with the same name is already running, it is stopped first.
    ///
    /// # Errors
    /// Returns `AiError` if the server cannot be started or initialized.
    pub async fn start_server(
        &mut self,
        name: &str,
        config: &McpServerConfig,
    ) -> Result<Vec<McpToolDefinition>, AiError> {
        // Stop existing server with the same name
        if self.clients.contains_key(name) {
            self.stop_server(name).await?;
        }

        let transport: Box<dyn McpTransportTrait> = match config.transport {
            minal_config::McpTransport::Stdio => {
                let command = config.command.as_deref().ok_or_else(|| {
                    AiError::McpTransport(format!("Server '{name}' has no command"))
                })?;
                Box::new(StdioTransport::new(command, &config.args, &config.env).await?)
            }
            minal_config::McpTransport::Sse => {
                let url = config
                    .url
                    .as_deref()
                    .ok_or_else(|| AiError::McpTransport(format!("Server '{name}' has no URL")))?;
                Box::new(SseTransport::new(url))
            }
        };

        let mut client = McpClient::new(transport);
        client.initialize().await?;

        let tools = client.list_tools().await?;
        let tool_count = self.registry.register_tools(name, tools.clone());

        tracing::info!(server = name, tools = tool_count, "MCP server started");
        self.clients.insert(name.to_string(), client);
        self.server_configs.insert(name.to_string(), config.clone());

        Ok(tools)
    }

    /// Stops an MCP server.
    ///
    /// # Errors
    /// Returns `AiError` if the server cannot be shut down cleanly.
    pub async fn stop_server(&mut self, name: &str) -> Result<(), AiError> {
        self.registry.unregister_server(name);
        self.server_configs.remove(name);
        if let Some(mut client) = self.clients.remove(name) {
            client.shutdown().await?;
        }
        tracing::info!(server = name, "MCP server stopped");
        Ok(())
    }

    /// Stops all MCP servers.
    pub async fn stop_all(&mut self) {
        let names: Vec<String> = self.clients.keys().cloned().collect();
        for name in names {
            if let Err(e) = self.stop_server(&name).await {
                tracing::warn!(server = name, error = %e, "Error stopping MCP server");
            }
        }
        self.server_configs.clear();
    }

    /// Checks if a server is running.
    pub fn is_server_running(&self, name: &str) -> bool {
        self.clients.contains_key(name)
    }

    /// Calls a tool by name, routing to the appropriate server.
    ///
    /// # Errors
    /// Returns `AiError::McpToolNotFound` if the tool is not registered, or
    /// `AiError::McpTransport` if the server is not running.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolCallResult, AiError> {
        let (server_name, _) = self
            .registry
            .get_tool(tool_name)
            .ok_or_else(|| AiError::McpToolNotFound(tool_name.to_string()))?;
        let server_name = server_name.to_string();

        let client = self.clients.get(&server_name).ok_or_else(|| {
            AiError::McpTransport(format!("Server '{server_name}' is not running"))
        })?;

        let timeout_secs = self
            .server_configs
            .get(&server_name)
            .map_or(30, |c| c.tool_timeout_secs);

        tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            client.call_tool(tool_name, arguments),
        )
        .await
        .map_err(|_| AiError::Timeout)?
    }

    /// Returns the tool registry (for listing tools).
    pub fn registry(&self) -> &McpToolRegistry {
        &self.registry
    }

    /// Returns all registered tools with their server names.
    pub fn all_tools(&self) -> Vec<(String, McpToolDefinition)> {
        self.registry
            .list_tools()
            .into_iter()
            .map(|(server, def)| (server.to_string(), def.clone()))
            .collect()
    }
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}
