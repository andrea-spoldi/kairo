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
    pub async fn connect(base_url: &str) -> Result<(Self, Vec<McpTool>)> {
        let http = reqwest::Client::new();
        let mut url = base_url.trim_end_matches('/').to_string();
        if let Ok(parsed) = reqwest::Url::parse(&url) {
            if parsed.path() == "/" || parsed.path().is_empty() {
                url.push_str("/mcp");
            }
        }
        let client = Self {
            http,
            base_url: url,
            next_id: AtomicU64::new(1),
        };
        client.initialize().await?;
        let tools = client.list_tools().await?;
        Ok((client, tools))
    }

    /// Send a JSON-RPC 2.0 request and return the `result` field.
    ///
    /// Handles both plain-JSON and SSE (text/event-stream) responses, as
    /// permitted by the MCP Streamable-HTTP transport spec.
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
            // Tell the server we accept both transports.
            .header("accept", "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .context("MCP: HTTP request failed")?;

        if !resp.status().is_success() {
            let s = resp.status();
            let t = resp.text().await.unwrap_or_default();
            bail!("MCP {s}: {t}");
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let v: Value = if content_type.contains("text/event-stream") {
            let text = resp.text().await.context("MCP: failed to read SSE body")?;
            parse_sse_json(&text)?
        } else {
            resp.json().await.context("MCP: invalid JSON response")?
        };

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

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Extract the first JSON object from an SSE body.
///
/// SSE lines look like:
/// ```text
/// event: message
/// data: {"jsonrpc":"2.0","id":1,"result":{...}}
/// ```
/// We scan for the first `data:` line that parses as JSON and return it.
fn parse_sse_json(sse_body: &str) -> Result<Value> {
    for line in sse_body.lines() {
        let data = if let Some(d) = line.strip_prefix("data: ") {
            d.trim()
        } else if let Some(d) = line.strip_prefix("data:") {
            d.trim()
        } else {
            continue;
        };

        if data.is_empty() {
            continue;
        }

        if let Ok(v) = serde_json::from_str::<Value>(data) {
            return Ok(v);
        }
    }
    bail!("MCP: no JSON data found in SSE response")
}
