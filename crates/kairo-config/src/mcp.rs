use serde::{Deserialize, Serialize};

/// Kubernetes MCP server configuration.
///
/// When `enabled` is true and `server_url` is set, the AI agent gains access
/// to live cluster tools through the Model Context Protocol server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
    /// URL of the running kubernetes-mcp-server (e.g. `http://localhost:8811`).
    pub server_url: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: "http://localhost:8811".into(),
        }
    }
}
