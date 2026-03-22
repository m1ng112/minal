//! MCP client for communicating with MCP servers.

use std::sync::atomic::{AtomicU64, Ordering};

use super::transport::McpTransportTrait;
use super::types::{
    JsonRpcRequest, McpInitializeResult, McpServerInfo, McpToolCallResult, McpToolDefinition,
};
use crate::error::AiError;

/// MCP client connection state.
#[derive(Debug, Clone, PartialEq)]
pub enum McpConnectionState {
    Disconnected,
    Initializing,
    Ready,
    Error(String),
    Closed,
}

/// Client for a single MCP server.
pub struct McpClient {
    transport: Box<dyn McpTransportTrait>,
    state: McpConnectionState,
    request_id: AtomicU64,
    server_info: Option<McpServerInfo>,
}

impl McpClient {
    /// Creates a new MCP client with the given transport.
    pub fn new(transport: Box<dyn McpTransportTrait>) -> Self {
        Self {
            transport,
            state: McpConnectionState::Disconnected,
            request_id: AtomicU64::new(1),
            server_info: None,
        }
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the current connection state.
    pub fn state(&self) -> &McpConnectionState {
        &self.state
    }

    /// Returns server info if available.
    pub fn server_info(&self) -> Option<&McpServerInfo> {
        self.server_info.as_ref()
    }

    /// Performs the MCP initialize handshake.
    ///
    /// # Errors
    /// Returns `AiError::McpProtocol` if initialization fails.
    pub async fn initialize(&mut self) -> Result<McpInitializeResult, AiError> {
        self.state = McpConnectionState::Initializing;

        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "minal",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let request = JsonRpcRequest::new(self.next_id(), "initialize", Some(params));
        let response = self.transport.send(&request).await.inspect_err(|e| {
            self.state = McpConnectionState::Error(e.to_string());
        })?;

        if let Some(error) = response.error {
            let msg = format!("Initialize failed: {} ({})", error.message, error.code);
            self.state = McpConnectionState::Error(msg.clone());
            return Err(AiError::McpProtocol(msg));
        }

        let result_value = response.result.ok_or_else(|| {
            let msg = "Initialize response has no result".to_string();
            self.state = McpConnectionState::Error(msg.clone());
            AiError::McpProtocol(msg)
        })?;

        let init_result: McpInitializeResult =
            serde_json::from_value(result_value).map_err(|e| {
                let msg = format!("Failed to parse initialize result: {e}");
                self.state = McpConnectionState::Error(msg.clone());
                AiError::McpProtocol(msg)
            })?;

        self.server_info = Some(init_result.server_info.clone());

        // Send initialized notification
        let notification = JsonRpcRequest::notification("notifications/initialized", None);
        self.transport
            .notify(&notification)
            .await
            .inspect_err(|e| {
                self.state = McpConnectionState::Error(e.to_string());
            })?;

        self.state = McpConnectionState::Ready;
        tracing::info!(
            server = init_result.server_info.name,
            version = init_result.server_info.version,
            "MCP server initialized"
        );

        Ok(init_result)
    }

    /// Lists available tools from the server.
    ///
    /// # Errors
    /// Returns `AiError::McpProtocol` if the client is not initialized or the request fails.
    pub async fn list_tools(&self) -> Result<Vec<McpToolDefinition>, AiError> {
        if self.state != McpConnectionState::Ready {
            return Err(AiError::McpProtocol("Client not initialized".to_string()));
        }

        let request = JsonRpcRequest::new(self.next_id(), "tools/list", None);
        let response = self.transport.send(&request).await?;

        if let Some(error) = response.error {
            return Err(AiError::McpProtocol(format!(
                "tools/list failed: {} ({})",
                error.message, error.code
            )));
        }

        let result = response
            .result
            .ok_or_else(|| AiError::McpProtocol("tools/list response has no result".to_string()))?;

        // The result should have a "tools" array
        let tools_value = result
            .get("tools")
            .cloned()
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        let tools: Vec<McpToolDefinition> = serde_json::from_value(tools_value)
            .map_err(|e| AiError::McpProtocol(format!("Failed to parse tools list: {e}")))?;

        tracing::debug!(count = tools.len(), "Listed MCP tools");
        Ok(tools)
    }

    /// Calls a tool on the server.
    ///
    /// # Errors
    /// Returns `AiError::McpProtocol` if the client is not initialized or the tool call fails.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolCallResult, AiError> {
        if self.state != McpConnectionState::Ready {
            return Err(AiError::McpProtocol("Client not initialized".to_string()));
        }

        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let request = JsonRpcRequest::new(self.next_id(), "tools/call", Some(params));
        let response = self.transport.send(&request).await?;

        if let Some(error) = response.error {
            return Err(AiError::McpProtocol(format!(
                "tools/call '{}' failed: {} ({})",
                name, error.message, error.code
            )));
        }

        let result = response.result.ok_or_else(|| {
            AiError::McpProtocol(format!("tools/call '{name}' response has no result"))
        })?;

        let tool_result: McpToolCallResult = serde_json::from_value(result)
            .map_err(|e| AiError::McpProtocol(format!("Failed to parse tool call result: {e}")))?;

        tracing::debug!(
            tool = name,
            is_error = tool_result.is_error,
            "MCP tool call completed"
        );
        Ok(tool_result)
    }

    /// Shuts down the client and closes the transport.
    ///
    /// # Errors
    /// Returns `AiError::McpTransport` if the transport cannot be closed.
    pub async fn shutdown(&mut self) -> Result<(), AiError> {
        self.state = McpConnectionState::Closed;
        self.transport.close().await?;
        tracing::info!("MCP client shut down");
        Ok(())
    }
}
