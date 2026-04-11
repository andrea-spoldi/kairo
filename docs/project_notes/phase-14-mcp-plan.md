# Phase 14 — MCP Server Integration

## What is this?

An MCP (Model Context Protocol) server is a small process that exposes structured tools over
stdio. Claude Desktop (or any MCP-compatible client) discovers those tools and calls them with
JSON arguments. The result: Claude can answer "which pods are crashing in the `payments`
namespace?" by actually querying your cluster rather than guessing.

This phase adds a **new standalone binary crate** — `kairo-mcp` — that wraps `kairo-core` and
exposes ~15 K8s tools over stdio transport.

---

## Architecture

```
Claude Desktop
     │  stdio (JSON-RPC 2.0)
     ▼
kairo-mcp binary
     │  async Rust function calls
     ▼
kairo-core (KubeClient, models, error types)
     │  kube-rs API
     ▼
Kubernetes cluster
```

`kairo-mcp` runs in its own tokio runtime and calls `kairo-core` directly — no IPC, no
channels, no shared memory. Each tool call is a single `async fn` that talks to the cluster
and returns JSON.

---

## New dependency: `rmcp`

`rmcp` is the official Rust MCP SDK published by `modelcontextprotocol` on crates.io.
It provides:

- `#[tool]` proc-macro to declare tools with automatic JSON schema generation
- `ServerHandler` trait
- Stdio transport (`transport-io` feature)
- All JSON-RPC 2.0 wiring

Additional deps needed: `schemars` (JSON schema for tool inputs), `serde`/`serde_json`
(already in core).

---

## Files to create or modify

| File | Action |
|------|--------|
| `Cargo.toml` | Add `kairo-mcp` to workspace members |
| `crates/kairo-mcp/Cargo.toml` | New crate |
| `crates/kairo-mcp/src/main.rs` | Server entry point |
| `crates/kairo-mcp/src/tools.rs` | All 15 tool implementations |
| `crates/kairo-core/src/client.rs` | Add one-shot `list_*` methods |
| `crates/kairo-core/src/models.rs` | Add `Serialize` to all summary structs |
| `crates/kairo-core/src/lib.rs` | Re-export new types/methods |
| `docs/project_notes/key_facts.md` | Update phase progress + approved deps |

---

## Step 1 — Workspace Cargo.toml

```toml
members = [
    "crates/kairo-core",
    "crates/kairo-ui",
    "crates/kairo-mcp",     # ← add
]
```

---

## Step 2 — `crates/kairo-mcp/Cargo.toml`

```toml
[package]
name = "kairo-mcp"
version.workspace = true
edition.workspace = true

[[bin]]
name = "kairo-mcp"
path = "src/main.rs"

[dependencies]
kairo-core  = { path = "../kairo-core" }
rmcp        = { version = "0.2", features = ["server", "transport-io"] }
schemars    = "0.8"
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
tokio       = { version = "1", features = ["full"] }
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

> Confirm the exact `rmcp` version with `cargo search rmcp` before implementing and pin
> accordingly.

---

## Step 3 — `kairo-core`: one-shot list methods

The existing client only has watcher-based data access. MCP tools need simple
request/response. Add to `client.rs`:

```rust
use kube::api::ListParams;
use crate::models::{ConfigMapSummary, DeploymentSummary, NodeSummary, PodSummary, ServiceSummary};

impl KubeClient {
    /// One-shot pod list (no watcher). namespace = None → all namespaces.
    pub async fn list_pods(&self, namespace: Option<&str>) -> Result<Vec<PodSummary>, CoreError> {
        let api: Api<Pod> = match namespace {
            Some(ns) => Api::namespaced(self.client.clone(), ns),
            None     => Api::all(self.client.clone()),
        };
        let list = api.list(&ListParams::default()).await?;
        Ok(list.into_iter().map(PodSummary::from).collect())
    }

    pub async fn list_deployments(&self, namespace: Option<&str>) -> Result<Vec<DeploymentSummary>, CoreError> { /* same pattern */ }
    pub async fn list_services(&self, namespace: Option<&str>) -> Result<Vec<ServiceSummary>, CoreError> { /* same pattern */ }
    pub async fn list_configmaps(&self, namespace: Option<&str>) -> Result<Vec<ConfigMapSummary>, CoreError> { /* same pattern */ }
    pub async fn list_nodes(&self) -> Result<Vec<NodeSummary>, CoreError> { /* Api::all */ }

    /// One-shot namespace list.
    pub async fn list_namespaces(&self) -> Result<Vec<String>, CoreError> {
        use k8s_openapi::api::core::v1::Namespace;
        let api: Api<Namespace> = Api::all(self.client.clone());
        let list = api.list(&ListParams::default()).await?;
        Ok(list.into_iter().filter_map(|ns| ns.metadata.name).collect())
    }

    /// Fetch last `lines` log lines from a pod container (non-streaming).
    pub async fn fetch_pod_logs_tail(
        &self, namespace: &str, pod: &str, container: Option<&str>, lines: u64,
    ) -> Result<String, CoreError> {
        use kube::api::LogParams;
        let api: Api<Pod> = Api::namespaced(self.client.clone(), namespace);
        let params = LogParams {
            container: container.map(str::to_string),
            tail_lines: Some(lines as i64),
            ..Default::default()
        };
        Ok(api.logs(pod, &params).await?)
    }
}
```

---

## Step 4 — `kairo-core/src/models.rs`: add `Serialize`

All summary structs already have `Deserialize` for tests. Add `Serialize` to:
`PodSummary`, `PodDetail`, `PodEvent`, `ContainerStatus`, `DeploymentSummary`,
`ServiceSummary`, `ConfigMapSummary`, `NodeSummary`, `ClusterEvent`.

One-word change per struct: `#[derive(Debug, Clone, Serialize, Deserialize)]`

---

## Step 5 — `crates/kairo-mcp/src/main.rs`

```rust
mod tools;

use rmcp::{ServiceExt, transport::stdio};
use tools::KairoServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("kairo_mcp=debug,kairo_core=info")
        .with_writer(std::io::stderr)   // stdout is reserved for MCP JSON-RPC
        .init();

    let client = kairo_core::KubeClient::try_default().await?;
    let server  = KairoServer { client };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

All tracing output goes to **stderr** — stdout carries the MCP JSON-RPC framing.

---

## Step 6 — `crates/kairo-mcp/src/tools.rs` — 15 tools

### Server struct

```rust
use kairo_core::KubeClient;

pub struct KairoServer {
    pub client: KubeClient,
}
```

### Input types (with JSON schema)

```rust
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NsOpt    { pub namespace: Option<String> }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NsName   { pub namespace: String, pub name: String }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeName { pub name: String }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct LogsInput {
    pub namespace: String,
    pub pod:       String,
    pub container: Option<String>,
    #[schemars(description = "Number of log lines to return (default 100, max 500)")]
    pub lines:     Option<u64>,
}
```

### Tools

```rust
#[tool(tool_box)]
impl KairoServer {

    #[tool(description = "List all kubeconfig contexts available on this machine")]
    async fn list_contexts(&self) -> Result<CallToolResult, McpError> {
        json_result(KubeClient::list_contexts()?)
    }

    #[tool(description = "List namespaces in the current cluster")]
    async fn list_namespaces(&self) -> Result<CallToolResult, McpError> {
        json_result(self.client.list_namespaces().await?)
    }

    #[tool(description = "List pods, optionally filtered by namespace")]
    async fn list_pods(&self, #[tool(aggr)] input: NsOpt) -> Result<CallToolResult, McpError> {
        json_result(self.client.list_pods(input.namespace.as_deref()).await?)
    }

    #[tool(description = "Get detailed info (conditions, containers, events) for a specific pod")]
    async fn get_pod(&self, #[tool(aggr)] input: NsName) -> Result<CallToolResult, McpError> {
        let mut detail = self.client.fetch_pod_detail(&input.namespace, &input.name).await?;
        if let Ok(evs) = self.client.fetch_pod_events(&input.namespace, &input.name).await {
            detail.events = evs;
        }
        json_result(detail)
    }

    #[tool(description = "Get raw YAML for a pod")]
    async fn get_pod_yaml(&self, #[tool(aggr)] input: NsName) -> Result<CallToolResult, McpError> {
        text_result(self.client.fetch_pod_yaml(&input.namespace, &input.name).await?)
    }

    #[tool(description = "Get the last N log lines from a pod container (default 100, max 500)")]
    async fn get_pod_logs(&self, #[tool(aggr)] input: LogsInput) -> Result<CallToolResult, McpError> {
        let lines = input.lines.unwrap_or(100).min(500);
        text_result(self.client.fetch_pod_logs_tail(
            &input.namespace, &input.pod, input.container.as_deref(), lines,
        ).await?)
    }

    #[tool(description = "List deployments, optionally filtered by namespace")]
    async fn list_deployments(&self, #[tool(aggr)] input: NsOpt) -> Result<CallToolResult, McpError> {
        json_result(self.client.list_deployments(input.namespace.as_deref()).await?)
    }

    #[tool(description = "Get raw YAML for a Deployment")]
    async fn get_deployment_yaml(&self, #[tool(aggr)] input: NsName) -> Result<CallToolResult, McpError> {
        text_result(self.client.fetch_deployment_yaml(&input.namespace, &input.name).await?)
    }

    #[tool(description = "List services, optionally filtered by namespace")]
    async fn list_services(&self, #[tool(aggr)] input: NsOpt) -> Result<CallToolResult, McpError> {
        json_result(self.client.list_services(input.namespace.as_deref()).await?)
    }

    #[tool(description = "Get raw YAML for a Service")]
    async fn get_service_yaml(&self, #[tool(aggr)] input: NsName) -> Result<CallToolResult, McpError> {
        text_result(self.client.fetch_service_yaml(&input.namespace, &input.name).await?)
    }

    #[tool(description = "List ConfigMaps, optionally filtered by namespace")]
    async fn list_configmaps(&self, #[tool(aggr)] input: NsOpt) -> Result<CallToolResult, McpError> {
        json_result(self.client.list_configmaps(input.namespace.as_deref()).await?)
    }

    #[tool(description = "Get raw YAML for a ConfigMap")]
    async fn get_configmap_yaml(&self, #[tool(aggr)] input: NsName) -> Result<CallToolResult, McpError> {
        text_result(self.client.fetch_configmap_yaml(&input.namespace, &input.name).await?)
    }

    #[tool(description = "List all nodes with CPU/memory capacity and allocatable resources")]
    async fn list_nodes(&self) -> Result<CallToolResult, McpError> {
        json_result(self.client.list_nodes().await?)
    }

    #[tool(description = "Get raw YAML for a Node")]
    async fn get_node_yaml(&self, #[tool(aggr)] input: NodeName) -> Result<CallToolResult, McpError> {
        text_result(self.client.fetch_node_yaml(&input.name).await?)
    }

    #[tool(description = "Summarize the cluster: context, node count, namespace count, pod phase counts")]
    async fn describe_cluster(&self) -> Result<CallToolResult, McpError> {
        let (pods, nodes, namespaces) = tokio::try_join!(
            self.client.list_pods(None),
            self.client.list_nodes(),
            self.client.list_namespaces(),
        )?;
        let running = pods.iter().filter(|p| p.status == "Running").count();
        let pending = pods.iter().filter(|p| p.status == "Pending").count();
        let failed  = pods.len().saturating_sub(running + pending);
        json_result(serde_json::json!({
            "context":    self.client.context,
            "nodes":      nodes.len(),
            "namespaces": namespaces.len(),
            "pods": { "total": pods.len(), "running": running, "pending": pending, "failed": failed }
        }))
    }
}
```

### Helpers

```rust
fn json_result(val: impl serde::Serialize) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![
        Content::text(
            serde_json::to_string_pretty(&val)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        )
    ]))
}

fn text_result(s: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(s)]))
}
```

### Error bridge

```rust
impl From<kairo_core::CoreError> for McpError {
    fn from(e: kairo_core::CoreError) -> Self {
        McpError::internal_error(e.to_string(), None)
    }
}
```

---

## Step 7 — Claude Desktop configuration

After `cargo build -p kairo-mcp`, register the server:

```json
// ~/Library/Application Support/Claude/claude_desktop_config.json  (macOS)
// ~/.config/Claude/claude_desktop_config.json                       (Linux)
{
  "mcpServers": {
    "kairo": {
      "command": "/path/to/target/release/kairo-mcp"
    }
  }
}
```

No environment variables needed — `kairo-mcp` reads `~/.kube/config` automatically.

---

## Verification

```bash
cargo check                          # all three crates compile
cargo clippy -- -D warnings          # clean
cargo build -p kairo-mcp             # binary produced

# Smoke-test without a cluster (should print server info to stderr, wait on stdin):
./target/debug/kairo-mcp

# Send a tools/list call:
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}\n' \
  | ./target/debug/kairo-mcp

# With a live cluster — describe the cluster:
printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}\n{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"describe_cluster","arguments":{}}}\n' \
  | ./target/debug/kairo-mcp
```

---

## What this phase does NOT include

- No UI changes — the MCP server is a completely separate process
- No multi-context switching mid-session (server starts with the current-context and keeps it)
- No streaming log tool (tail-only; streaming over MCP requires SSE transport — future phase)

---

## Summary table

| Step | File | Change |
|------|------|--------|
| 1 | `Cargo.toml` | Add `kairo-mcp` workspace member |
| 2 | `crates/kairo-mcp/Cargo.toml` | New crate with `rmcp`, `schemars` |
| 3 | `kairo-core/src/client.rs` | `list_pods/deployments/services/configmaps/nodes/namespaces` + `fetch_pod_logs_tail` |
| 4 | `kairo-core/src/models.rs` | Add `Serialize` to all summary structs |
| 5 | `kairo-core/src/lib.rs` | Re-export updated types |
| 6 | `kairo-mcp/src/main.rs` | Tokio entry point, stdio transport |
| 7 | `kairo-mcp/src/tools.rs` | 15 tool implementations + error bridge |
| 8 | `docs/project_notes/key_facts.md` | Update phase progress + approved deps |
