//! MCP (Model Context Protocol) client implementation.
//!
//! Provides JSON-RPC 2.0 communication with MCP servers over stdio
//! and SSE transports, tool discovery, and lifecycle management.

pub mod client;
pub mod manager;
pub mod registry;
pub mod transport;
pub mod types;

pub use client::McpClient;
pub use manager::McpServerManager;
pub use registry::McpToolRegistry;
pub use types::{McpToolCallResult, McpToolDefinition};
