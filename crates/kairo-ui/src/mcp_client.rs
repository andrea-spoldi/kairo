//! MCP (Model Context Protocol) HTTP client.
//!
//! Implements the Streamable-HTTP transport: JSON-RPC 2.0 messages sent as
//! HTTP POST requests to the server base URL.
//!
//! Initialization handshake (per MCP spec):
//!   1. POST `initialize` → server responds with capabilities + `Mcp-Session-Id` header
//!   2. POST `notifications/initialized` (include session ID) → server marks session ready
//!   3. All subsequent requests include `Mcp-Session-Id`; without it the server
//!      rejects them as "invalid during session initialization".

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
    /// Session ID returned by the server in the `initialize` response header
    /// (`Mcp-Session-Id`).  Must be echoed in every subsequent request.
    session_id: Option<String>,
}

impl McpClient {
    /// Connect to the MCP server: run the full initialize handshake and fetch
    /// the tool list.  Returns `(client, tools)` on success.
    pub async fn connect(base_url: &str) -> Result<(Self, Vec<McpTool>)> {
        let http = reqwest::Client::new();

        // Normalize URL: many kubernetes-mcp-server deployments mount at /mcp.
        let mut url = base_url.trim_end_matches('/').to_string();
        if let Ok(parsed) = reqwest::Url::parse(&url) {
            if parsed.path() == "/" || parsed.path().is_empty() {
                url.push_str("/mcp");
            }
        }

        // Steps 1 + 2: initialize → capture session ID → notifications/initialized.
        let session_id = do_initialize(&http, &url).await?;

        let client = Self {
            http,
            base_url: url,
            next_id: AtomicU64::new(2), // id=1 was consumed by initialize
            session_id,
        };

        // Step 3: fetch the tool list.
        let tools = client.list_tools().await?;
        Ok((client, tools))
    }

    // ── Private ────────────────────────────────────────────────────────────────

    /// Send a JSON-RPC 2.0 request and return its `result` field.
    /// Attaches `Mcp-Session-Id` if one was obtained during initialization.
    async fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut req = self
            .http
            .post(&self.base_url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");

        if let Some(ref sid) = self.session_id {
            req = req.header("mcp-session-id", sid);
        }

        let resp = req
            .json(&body)
            .send()
            .await
            .context("MCP: HTTP request failed")?;

        if !resp.status().is_success() {
            let s = resp.status();
            let t = resp.text().await.unwrap_or_default();
            bail!("MCP {s}: {t}");
        }

        let v = parse_response(resp).await?;

        if let Some(err) = v.get("error") {
            bail!("MCP error: {err}");
        }
        Ok(v["result"].clone())
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

// ── Initialization ─────────────────────────────────────────────────────────────

/// Run the MCP `initialize` → `notifications/initialized` handshake.
///
/// Done outside `rpc()` so we can capture the `Mcp-Session-Id` response header
/// before the body is consumed, and include it in the follow-up notification.
async fn do_initialize(http: &reqwest::Client, url: &str) -> Result<Option<String>> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "clientInfo": {
                "name": "kairo",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {}
        }
    });

    let resp = http
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .context("MCP: initialize request failed")?;

    if !resp.status().is_success() {
        let s = resp.status();
        let t = resp.text().await.unwrap_or_default();
        bail!("MCP initialize {s}: {t}");
    }

    // Capture session ID from headers BEFORE consuming the body.
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let v = parse_response(resp).await?;
    if let Some(err) = v.get("error") {
        bail!("MCP initialize error: {err}");
    }

    // Send the `notifications/initialized` notification to complete the
    // handshake.  This is one-way (no `id`); include the session ID so the
    // server associates it with the just-created session.
    let notif = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let mut req = http
        .post(url)
        .header("content-type", "application/json");
    if let Some(ref sid) = session_id {
        req = req.header("mcp-session-id", sid);
    }
    let _ = req.json(&notif).send().await;

    Ok(session_id)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Parse a response that may be plain JSON or an SSE stream.
async fn parse_response(resp: reqwest::Response) -> Result<Value> {
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("text/event-stream") {
        let text = resp.text().await.context("MCP: failed to read SSE body")?;
        parse_sse_json(&text)
    } else {
        resp.json().await.context("MCP: invalid JSON response")
    }
}

/// Extract the first JSON object from an SSE body.
///
/// SSE lines look like:
/// ```text
/// event: message
/// data: {"jsonrpc":"2.0","id":1,"result":{...}}
/// ```
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
