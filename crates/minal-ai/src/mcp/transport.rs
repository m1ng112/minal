//! MCP transport implementations.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::types::{JsonRpcRequest, JsonRpcResponse};
use crate::error::AiError;

/// Trait for MCP transports.
#[async_trait::async_trait]
pub trait McpTransportTrait: Send + Sync {
    /// Send a request and receive a response.
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, AiError>;

    /// Send a notification (no response expected).
    async fn notify(&self, request: &JsonRpcRequest) -> Result<(), AiError>;

    /// Close the transport.
    async fn close(&self) -> Result<(), AiError>;

    /// Check if the transport is still alive.
    fn is_alive(&self) -> bool;
}

/// Stdio transport - communicates with MCP server via child process stdin/stdout.
pub struct StdioTransport {
    inner: Mutex<StdioTransportInner>,
    alive_flag: Arc<AtomicBool>,
}

struct StdioTransportInner {
    child: Child,
    stdin: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    alive: bool,
}

impl StdioTransport {
    /// Starts a new MCP server process and creates a transport.
    ///
    /// # Errors
    /// Returns `AiError::McpTransport` if the process cannot be started.
    pub async fn new(
        command: &str,
        args: &[String],
        env: &BTreeMap<String, String>,
    ) -> Result<Self, AiError> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            AiError::McpTransport(format!("Failed to start MCP server '{command}': {e}"))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            AiError::McpTransport("Failed to get stdin of MCP server".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AiError::McpTransport("Failed to get stdout of MCP server".to_string())
        })?;
        let reader = BufReader::new(stdout);

        tracing::info!(command, "MCP stdio transport started");

        let alive_flag = Arc::new(AtomicBool::new(true));
        Ok(Self {
            inner: Mutex::new(StdioTransportInner {
                child,
                stdin,
                reader,
                alive: true,
            }),
            alive_flag,
        })
    }
}

#[async_trait::async_trait]
impl McpTransportTrait for StdioTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, AiError> {
        let mut inner = self.inner.lock().await;
        if !inner.alive {
            return Err(AiError::McpTransport("Transport is closed".to_string()));
        }

        // Serialize and send the request
        let mut json = serde_json::to_string(request)
            .map_err(|e| AiError::McpProtocol(format!("Failed to serialize request: {e}")))?;
        json.push('\n');

        inner.stdin.write_all(json.as_bytes()).await.map_err(|e| {
            inner.alive = false;
            self.alive_flag.store(false, Ordering::Relaxed);
            AiError::McpTransport(format!("Failed to write to MCP server: {e}"))
        })?;
        inner.stdin.flush().await.map_err(|e| {
            inner.alive = false;
            self.alive_flag.store(false, Ordering::Relaxed);
            AiError::McpTransport(format!("Failed to flush to MCP server: {e}"))
        })?;

        // Read the response line
        let mut line = String::new();
        let n = inner.reader.read_line(&mut line).await.map_err(|e| {
            inner.alive = false;
            self.alive_flag.store(false, Ordering::Relaxed);
            AiError::McpTransport(format!("Failed to read from MCP server: {e}"))
        })?;

        if n == 0 {
            inner.alive = false;
            self.alive_flag.store(false, Ordering::Relaxed);
            return Err(AiError::McpTransport(
                "MCP server closed connection".to_string(),
            ));
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim())
            .map_err(|e| AiError::McpProtocol(format!("Invalid JSON-RPC response: {e}")))?;

        Ok(response)
    }

    async fn notify(&self, request: &JsonRpcRequest) -> Result<(), AiError> {
        let mut inner = self.inner.lock().await;
        if !inner.alive {
            return Err(AiError::McpTransport("Transport is closed".to_string()));
        }

        let mut json = serde_json::to_string(request)
            .map_err(|e| AiError::McpProtocol(format!("Failed to serialize notification: {e}")))?;
        json.push('\n');

        inner.stdin.write_all(json.as_bytes()).await.map_err(|e| {
            inner.alive = false;
            self.alive_flag.store(false, Ordering::Relaxed);
            AiError::McpTransport(format!("Failed to write notification to MCP server: {e}"))
        })?;
        inner.stdin.flush().await.map_err(|e| {
            inner.alive = false;
            self.alive_flag.store(false, Ordering::Relaxed);
            AiError::McpTransport(format!("Failed to flush notification to MCP server: {e}"))
        })?;

        Ok(())
    }

    async fn close(&self) -> Result<(), AiError> {
        let mut inner = self.inner.lock().await;
        inner.alive = false;
        self.alive_flag.store(false, Ordering::Relaxed);
        // Try to kill the child process
        if let Err(e) = inner.child.kill().await {
            tracing::debug!("Error killing MCP server process: {e}");
        }
        tracing::info!("MCP stdio transport closed");
        Ok(())
    }

    fn is_alive(&self) -> bool {
        self.alive_flag.load(Ordering::Relaxed)
    }
}

/// SSE transport - communicates with MCP server via HTTP.
/// This is a minimal stub implementation.
pub struct SseTransport {
    _url: String,
}

impl SseTransport {
    /// Creates a new SSE transport (stub).
    pub fn new(url: &str) -> Self {
        Self {
            _url: url.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl McpTransportTrait for SseTransport {
    async fn send(&self, _request: &JsonRpcRequest) -> Result<JsonRpcResponse, AiError> {
        Err(AiError::McpTransport(
            "SSE transport not yet implemented".to_string(),
        ))
    }

    async fn notify(&self, _request: &JsonRpcRequest) -> Result<(), AiError> {
        Err(AiError::McpTransport(
            "SSE transport not yet implemented".to_string(),
        ))
    }

    async fn close(&self) -> Result<(), AiError> {
        Ok(())
    }

    fn is_alive(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_transport_not_implemented() {
        let transport = SseTransport::new("http://localhost:3000");
        assert!(!transport.is_alive());
    }
}
