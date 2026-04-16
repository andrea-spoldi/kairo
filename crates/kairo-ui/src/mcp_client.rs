//! MCP (Model Context Protocol) HTTP client.
//!
//! Implements the Streamable-HTTP transport: JSON-RPC 2.0 messages sent as
//! HTTP POST requests to the server base URL.

use anyhow::{bail, Context as _, Result};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

// ── Types ──────────────────────────────────────────────────────────────────────

/// A tool exposed by the MCP server.
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    /// JSON Schema object describing the tool's input parameters.
    pub input_schema: Value,
}

// ── Client ─────────────────────────────────────────────────────────────────────

pub struct McpClient {
    http: reqwest::Client,
    base_url: String,
    next_id: AtomicU64,
}

impl McpClient {
    /// Connect to the MCP server: run the `initialize` handshake and fetch the
    /// tool list.  Returns `(client, tools)` on success.
    pub async fn connect(base_url: &str) -> Result<(Self, Vec<McpTool>)> {
        let http = reqwest::Client::new();
        let client = Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            next_id: AtomicU64::new(1),
        };
        client.initialize().await?;
        let tools = client.list_tools().await?;
        Ok((client, tools))
    }

    /// Send a JSON-RPC 2.0 request and return the `result` field.
    async fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let resp = self
            .http
            .post(&self.base_url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("MCP: HTTP request failed")?;

        if !resp.status().is_success() {
            let s = resp.status();
            let t = resp.text().await.unwrap_or_default();
            bail!("MCP {s}: {t}");
        }

        let v: Value = resp.json().await.context("MCP: invalid JSON response")?;
        if let Some(err) = v.get("error") {
            bail!("MCP error: {err}");
        }
        Ok(v["result"].clone())
    }

    async fn initialize(&self) -> Result<()> {
        self.rpc(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "clientInfo": {
                    "name": "kairo",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {}
            }),
        )
        .await?;

        // Send the `notifications/initialized` notification (fire-and-forget —
        // some servers require it, others ignore it).
        let _ = self
            .http
            .post(&self.base_url)
            .header("content-type", "application/json")
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .send()
            .await;

        Ok(())
    }

    async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let result = self.rpc("tools/list", json!({})).await?;
        let tools = result["tools"].as_array().cloned().unwrap_or_default();
        Ok(tools
            .iter()
            .map(|t| McpTool {
                name: t["name"].as_str().unwrap_or("").to_string(),
                description: t["description"].as_str().unwrap_or("").to_string(),
                input_schema: t["inputSchema"].clone(),
            })
            .collect())
    }

    /// Call a named tool with the given arguments and return its text output.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String> {
        let result = self
            .rpc("tools/call", json!({ "name": name, "arguments": args }))
            .await?;

        // MCP content format: { "content": [{ "type": "text", "text": "…" }] }
        if let Some(content) = result["content"].as_array() {
            let text = content
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(text);
        }

        Ok(result.to_string())
    }
}
