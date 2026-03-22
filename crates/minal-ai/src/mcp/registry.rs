//! MCP tool registry aggregating tools from multiple servers.

use std::collections::HashMap;

use super::types::McpToolDefinition;

/// Aggregates tools from multiple MCP servers.
pub struct McpToolRegistry {
    /// Maps tool name -> (server_name, definition)
    tools: HashMap<String, (String, McpToolDefinition)>,
}

impl McpToolRegistry {
    /// Creates a new empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Registers tools from a server. Returns count of newly registered tools.
    pub fn register_tools(&mut self, server_name: &str, tools: Vec<McpToolDefinition>) -> usize {
        let mut count = 0;
        for tool in tools {
            if self.tools.contains_key(&tool.name) {
                tracing::warn!(
                    tool = tool.name,
                    server = server_name,
                    "MCP tool name conflict, skipping"
                );
                continue;
            }
            self.tools
                .insert(tool.name.clone(), (server_name.to_string(), tool));
            count += 1;
        }
        tracing::info!(server = server_name, count, "Registered MCP tools");
        count
    }

    /// Removes all tools from a server.
    pub fn unregister_server(&mut self, server_name: &str) {
        self.tools.retain(|_, (s, _)| s != server_name);
        tracing::info!(server = server_name, "Unregistered MCP server tools");
    }

    /// Looks up a tool by name.
    pub fn get_tool(&self, name: &str) -> Option<(&str, &McpToolDefinition)> {
        self.tools.get(name).map(|(s, d)| (s.as_str(), d))
    }

    /// Returns all registered tools sorted by tool name.
    pub fn list_tools(&self) -> Vec<(&str, &McpToolDefinition)> {
        let mut tools: Vec<_> = self.tools.values().map(|(s, d)| (s.as_str(), d)).collect();
        tools.sort_by_key(|(_, d)| d.name.as_str());
        tools
    }

    /// Formats tool descriptions for inclusion in AI prompts.
    pub fn format_tools_for_ai(&self) -> String {
        if self.tools.is_empty() {
            return String::new();
        }
        let mut out = String::from("Available MCP tools:\n");
        for (server, def) in self.list_tools() {
            out.push_str(&format!("- {} (server: {})", def.name, server));
            if let Some(ref desc) = def.description {
                out.push_str(&format!(": {desc}"));
            }
            out.push('\n');
            if let Some(ref schema) = def.input_schema {
                out.push_str(&format!(
                    "  Input: {}\n",
                    serde_json::to_string(schema).unwrap_or_default()
                ));
            }
        }
        out
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns true if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for McpToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool(name: &str, desc: &str) -> McpToolDefinition {
        McpToolDefinition {
            name: name.to_string(),
            description: Some(desc.to_string()),
            input_schema: None,
        }
    }

    #[test]
    fn register_and_list() {
        let mut registry = McpToolRegistry::new();
        let tools = vec![
            make_tool("read_file", "Read a file"),
            make_tool("write_file", "Write a file"),
        ];
        let count = registry.register_tools("fs", tools);
        assert_eq!(count, 2);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn get_tool() {
        let mut registry = McpToolRegistry::new();
        registry.register_tools("fs", vec![make_tool("read_file", "Read a file")]);
        let (server, tool) = registry.get_tool("read_file").unwrap();
        assert_eq!(server, "fs");
        assert_eq!(tool.name, "read_file");
    }

    #[test]
    fn get_tool_not_found() {
        let registry = McpToolRegistry::new();
        assert!(registry.get_tool("nonexistent").is_none());
    }

    #[test]
    fn unregister_server() {
        let mut registry = McpToolRegistry::new();
        registry.register_tools("fs", vec![make_tool("read_file", "Read")]);
        registry.register_tools("db", vec![make_tool("query", "Query DB")]);
        assert_eq!(registry.len(), 2);
        registry.unregister_server("fs");
        assert_eq!(registry.len(), 1);
        assert!(registry.get_tool("read_file").is_none());
        assert!(registry.get_tool("query").is_some());
    }

    #[test]
    fn name_conflict() {
        let mut registry = McpToolRegistry::new();
        registry.register_tools("fs1", vec![make_tool("read_file", "First")]);
        let count = registry.register_tools("fs2", vec![make_tool("read_file", "Second")]);
        assert_eq!(count, 0); // Conflict - not registered
        let (server, _) = registry.get_tool("read_file").unwrap();
        assert_eq!(server, "fs1"); // First wins
    }

    #[test]
    fn format_for_ai() {
        let mut registry = McpToolRegistry::new();
        registry.register_tools("fs", vec![make_tool("read_file", "Read a file")]);
        let formatted = registry.format_tools_for_ai();
        assert!(formatted.contains("read_file"));
        assert!(formatted.contains("Read a file"));
        assert!(formatted.contains("server: fs"));
    }

    #[test]
    fn empty_registry() {
        let registry = McpToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.format_tools_for_ai(), "");
    }
}
