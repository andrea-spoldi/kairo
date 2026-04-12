# Phase 15 — AI Integration: Cluster Intelligence

## Vision

Embed an AI analysis engine directly inside Kairo. When the user selects a resource —
a crashing pod, a degraded deployment, a memory-pressured node — they can trigger
"Analyze with AI" and receive:

- A plain-English root cause hypothesis
- Severity assessment
- Bullet-point insights
- Actionable recommendations
- Ready-to-run `kubectl` commands for deeper investigation

The LLM never sees raw kubeconfig credentials. It only sees structured, sanitized
context assembled by the Context Builder.

---

## Full Data Flow

```
UI (GPUI)
  │  user clicks "Analyze" (button / keyboard shortcut)
  ▼
AiController  (kairo-ai crate)
  │
  ├─ ContextBuilder  →  assembles AnalysisContext (structured JSON)
  │       ↑ pulls from: PodDetail, logs tail, ClusterEvents, NodeSummary
  │
  ├─ PromptEngine   →  system_prompt (static, cacheable)
  │                 →  user_prompt   (dynamic, AnalysisContext as JSON)
  │
  └─ LlmProvider    →  POST /v1/messages  (Anthropic API, prompt caching)
          │
          ▼
    JSON response  { summary, root_cause, severity, insights,
                     recommendations, related_commands }
          │
  ResponseParser   →  AiAnalysis struct
          │
  KubeEvent::AiResult(AiAnalysis)
          │
  GPUI poll loop
          │
  AiPanel (new right-dock tab)  →  rendered sections
```

---

## Architecture: new `kairo-ai` crate

Three-crate workspace after this phase:

```
kairo-core  (K8s client + models — zero UI deps)
    ↓ depends on
kairo-ai    (AI engine — depends on kairo-core, zero UI deps)
    ↓ depends on
kairo-ui    (GPUI app — depends on both)
kairo-mcp   (MCP server binary — depends on kairo-core only)
```

**Hard rules (same as kairo-core):**
- `kairo-ai` must NEVER import `gpui`
- `kairo-ai` contains zero rendering logic
- All context assembly happens in `kairo-ai`; the UI only renders the result

---

## Files to create or modify

| File | Action |
|------|--------|
| `Cargo.toml` | Add `kairo-ai` workspace member |
| `crates/kairo-ai/Cargo.toml` | New crate |
| `crates/kairo-ai/src/lib.rs` | Public API surface |
| `crates/kairo-ai/src/context.rs` | `ContextBuilder` — assembles `AnalysisContext` |
| `crates/kairo-ai/src/prompts.rs` | System prompt constant + `PromptEngine` |
| `crates/kairo-ai/src/provider.rs` | `LlmProvider` — Anthropic HTTP client |
| `crates/kairo-ai/src/response.rs` | `ResponseParser` + `AiAnalysis` struct |
| `crates/kairo-ai/src/controller.rs` | `AiController` — orchestrates the pipeline |
| `crates/kairo-ai/src/error.rs` | `AiError` type |
| `crates/kairo-ai/src/config.rs` | `AiConfig` — API key, model, max tokens |
| `crates/kairo-core/src/models.rs` | Add `Serialize` to all summary structs |
| `crates/kairo-core/src/client.rs` | Add `fetch_pod_logs_tail` (reuse from Phase 14 plan) |
| `crates/kairo-ui/Cargo.toml` | Add `kairo-ai` dependency |
| `crates/kairo-ui/src/app.rs` | `KubeEvent::AiResult` + `AiController` init |
| `crates/kairo-ui/src/components/ai_panel.rs` | New `AiPanel` dock component |
| `crates/kairo-ui/src/components/mod.rs` | Register `ai_panel` module |

---

## Step 1 — Workspace `Cargo.toml`

```toml
members = [
    "crates/kairo-core",
    "crates/kairo-ai",      # ← add
    "crates/kairo-ui",
    "crates/kairo-mcp",
]
```

---

## Step 2 — `crates/kairo-ai/Cargo.toml`

```toml
[package]
name = "kairo-ai"
version.workspace = true
edition.workspace = true

[dependencies]
kairo-core    = { path = "../kairo-core" }
reqwest       = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
tokio         = { version = "1", features = ["full"] }
tracing       = "0.1"
thiserror     = "1"
```

`reqwest` 0.12 is the current stable; uses `rustls-tls` to stay consistent with kube-rs.
No `openssl` dependency added.

---

## Step 3 — `kairo-core/src/models.rs`: add `Serialize`

All summary structs need `serde::Serialize` so the Context Builder can serialize them
into the JSON user prompt. One-word change per struct:

```rust
// Before:
#[derive(Debug, Clone)]
pub struct PodSummary { ... }

// After:
#[derive(Debug, Clone, serde::Serialize)]
pub struct PodSummary { ... }
```

Apply to: `PodSummary`, `PodDetail`, `PodEvent`, `ContainerStatus`,
`DeploymentSummary`, `ServiceSummary`, `ConfigMapSummary`, `NodeSummary`, `ClusterEvent`.

---

## Step 4 — `kairo-ai/src/error.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("Anthropic API request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Anthropic API returned an error: {status} — {message}")]
    Api { status: u16, message: String },

    #[error("Failed to parse LLM response as JSON: {0}")]
    ParseError(String),

    #[error("AI features disabled: ANTHROPIC_API_KEY not set")]
    NotConfigured,

    #[error("Context assembly failed: {0}")]
    Context(String),
}
```

---

## Step 5 — `kairo-ai/src/config.rs`

```rust
/// Runtime configuration for the AI engine.
#[derive(Debug, Clone)]
pub struct AiConfig {
    /// Anthropic API key. Read from ANTHROPIC_API_KEY env var.
    pub api_key: String,
    /// Model to use. Default: claude-sonnet-4-6.
    pub model: String,
    /// Max tokens in the response. Default: 1024.
    pub max_tokens: u32,
}

impl AiConfig {
    /// Load from environment. Returns None if ANTHROPIC_API_KEY is not set.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        Some(Self {
            api_key,
            model: std::env::var("KAIRO_AI_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-6".to_string()),
            max_tokens: 1024,
        })
    }
}
```

---

## Step 6 — `kairo-ai/src/context.rs`: `ContextBuilder`

The Context Builder assembles a rich, structured `AnalysisContext` that becomes the
JSON user prompt. It deliberately filters noise (e.g. managed fields, resource
versions) to keep the prompt focused.

```rust
use kairo_core::models::{ClusterEvent, NodeSummary, PodDetail, PodSummary};
use serde::Serialize;

/// The analysis type determines which data fields are populated.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisContext {
    Pod(PodContext),
    Cluster(ClusterContext),
    Node(NodeContext),
}

#[derive(Debug, Clone, Serialize)]
pub struct PodContext {
    pub name: String,
    pub namespace: String,
    pub status: String,
    pub ready: String,
    pub restarts: i32,
    pub node: String,
    pub conditions: Vec<Condition>,
    pub containers: Vec<ContainerCtx>,
    pub recent_events: Vec<EventCtx>,
    /// Last N lines of logs from the primary container.
    pub log_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerCtx {
    pub name: String,
    pub image: String,
    pub ready: bool,
    pub restarts: i32,
    pub state: String,       // "running" | "waiting: <reason>" | "terminated: <reason>"
    pub last_state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Condition {
    pub type_: String,
    pub status: String,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventCtx {
    pub reason: String,
    pub message: String,
    pub count: i32,
    pub type_: String,   // "Warning" | "Normal"
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterContext {
    pub node_count: usize,
    pub namespace_count: usize,
    pub pod_summary: PodStats,
    pub recent_warnings: Vec<EventCtx>,
    pub node_pressure: Vec<NodePressure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeContext {
    pub name: String,
    pub status: String,
    pub roles: String,
    pub cpu_used_pct: f32,
    pub memory_used_pct: f32,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PodStats {
    pub total: usize,
    pub running: usize,
    pub pending: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodePressure {
    pub node: String,
    pub memory_pressure: bool,
    pub disk_pressure: bool,
    pub pid_pressure: bool,
}

/// Build a `PodContext` from a `PodDetail` + optional log tail + cluster events.
pub fn build_pod_context(
    detail: &PodDetail,
    log_tail: Option<String>,
    events: &[ClusterEvent],
) -> AnalysisContext {
    // ... map PodDetail fields into PodContext, filter to pod-relevant events
}

/// Build a `ClusterContext` from aggregate data.
pub fn build_cluster_context(
    pods: &[PodSummary],
    nodes: &[NodeSummary],
    events: &[ClusterEvent],
    namespace_count: usize,
) -> AnalysisContext {
    // ... aggregate stats, filter warnings only
}
```

---

## Step 7 — `kairo-ai/src/prompts.rs`: System Prompt + PromptEngine

### System prompt (static, fully cached)

The system prompt is a constant string embedded in the binary.
It is marked with `cache_control: ephemeral` in the API request so Anthropic caches
the tokenized version — reducing cost and latency on repeated calls.

```
You are an expert Kubernetes SRE analyst embedded in Kairo, a native Kubernetes IDE.

Your job is to analyze structured Kubernetes context data and return a precise,
actionable diagnosis. The context is provided as JSON.

You MUST respond with a single valid JSON object matching this exact schema:

{
  "summary":          "<1-2 sentence plain-English overview>",
  "root_cause":       "<null or concise root cause if a problem exists>",
  "severity":         "ok" | "warning" | "critical",
  "insights":         ["<bullet point>", ...],           // 2-5 items
  "recommendations":  ["<actionable step>", ...],        // 2-4 items
  "related_commands": ["kubectl ...", ...]               // 1-4 ready-to-run commands
}

Rules:
- Never invent data not present in the context.
- If no problem is detected, set severity to "ok" and root_cause to null.
- Keep each insight and recommendation under 120 characters.
- kubectl commands must reference the actual resource names and namespaces from context.
- Do not include markdown, backticks, or any text outside the JSON object.
```

### `PromptEngine`

```rust
pub struct PromptEngine;

impl PromptEngine {
    /// Render the dynamic user prompt from an AnalysisContext.
    pub fn user_prompt(ctx: &AnalysisContext) -> String {
        // Serialize context as pretty JSON, wrap in instruction header.
        let json = serde_json::to_string_pretty(ctx).unwrap_or_default();
        format!(
            "Analyze the following Kubernetes resource context and return your JSON diagnosis:\n\n{json}"
        )
    }
}
```

---

## Step 8 — `kairo-ai/src/provider.rs`: `LlmProvider`

Calls the Anthropic Messages API with prompt caching enabled on the system turn.

```rust
use reqwest::Client;
use serde_json::{json, Value};
use crate::{AiConfig, AiError};

pub struct LlmProvider {
    client: Client,
    config: AiConfig,
}

impl LlmProvider {
    pub fn new(config: AiConfig) -> Self {
        Self { client: Client::new(), config }
    }

    pub async fn complete(&self, system: &str, user: &str) -> Result<String, AiError> {
        let body = json!({
            "model": self.config.model,
            "max_tokens": self.config.max_tokens,
            "system": [
                {
                    "type": "text",
                    "text": system,
                    // Prompt caching: system prompt is static → always hits cache
                    "cache_control": { "type": "ephemeral" }
                }
            ],
            "messages": [
                { "role": "user", "content": user }
            ]
        });

        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .json(&body)
            .send()
            .await?;

        let status = resp.status().as_u16();
        let json: Value = resp.json().await?;

        if status != 200 {
            let msg = json["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            return Err(AiError::Api { status, message: msg });
        }

        json["content"][0]["text"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| AiError::ParseError("no text content in response".into()))
    }
}
```

---

## Step 9 — `kairo-ai/src/response.rs`: `AiAnalysis` + `ResponseParser`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysis {
    pub summary: String,
    pub root_cause: Option<String>,
    pub severity: Severity,
    pub insights: Vec<String>,
    pub recommendations: Vec<String>,
    pub related_commands: Vec<String>,
    /// Which resource triggered this analysis (for display purposes).
    pub resource_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Ok, Warning, Critical }

pub struct ResponseParser;

impl ResponseParser {
    pub fn parse(raw: &str, resource_label: String) -> Result<AiAnalysis, crate::AiError> {
        // Strip accidental markdown fences if present.
        let cleaned = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let mut analysis: AiAnalysis = serde_json::from_str(cleaned)
            .map_err(|e| crate::AiError::ParseError(e.to_string()))?;
        analysis.resource_label = resource_label;
        Ok(analysis)
    }
}
```

---

## Step 10 — `kairo-ai/src/controller.rs`: `AiController`

```rust
use crate::{
    AiConfig, AiError,
    context::{AnalysisContext, build_cluster_context, build_pod_context},
    prompts::{SYSTEM_PROMPT, PromptEngine},
    provider::LlmProvider,
    response::{AiAnalysis, ResponseParser},
};
use kairo_core::models::{ClusterEvent, NodeSummary, PodDetail, PodSummary};

pub struct AiController {
    provider: LlmProvider,
}

impl AiController {
    pub fn new(config: AiConfig) -> Self {
        Self { provider: LlmProvider::new(config) }
    }

    /// Analyze a specific pod. `log_tail` = last N log lines from primary container.
    pub async fn analyze_pod(
        &self,
        detail: &PodDetail,
        log_tail: Option<String>,
        events: &[ClusterEvent],
    ) -> Result<AiAnalysis, AiError> {
        let label = format!("Pod / {}/{}", detail.summary.namespace, detail.summary.name);
        let ctx = build_pod_context(detail, log_tail, events);
        self.run(ctx, label).await
    }

    /// Analyze overall cluster health.
    pub async fn analyze_cluster(
        &self,
        pods: &[PodSummary],
        nodes: &[NodeSummary],
        events: &[ClusterEvent],
        namespace_count: usize,
    ) -> Result<AiAnalysis, AiError> {
        let ctx = build_cluster_context(pods, nodes, events, namespace_count);
        self.run(ctx, "Cluster Overview".to_string()).await
    }

    async fn run(&self, ctx: AnalysisContext, label: String) -> Result<AiAnalysis, AiError> {
        let user_prompt = PromptEngine::user_prompt(&ctx);
        let raw = self.provider.complete(SYSTEM_PROMPT, &user_prompt).await?;
        ResponseParser::parse(&raw, label)
    }
}
```

---

## Step 11 — `kairo-ui`: `AiPanel` component

New panel rendered in the **right dock** alongside the YAML viewer — as a second tab.

```
Right dock tabs:
  [YAML]  [AI Insights]
```

### `AiPanel` states

```
┌─────────────────────────────────────────┐
│ AI Insights                             │
│─────────────────────────────────────────│
│  [idle]   Select a resource and         │
│           click Analyze to get insights │
│                                         │
│  [loading] ◌ Analyzing pod/my-app…      │
│                                         │
│  [result]                               │
│  ● Pod / default/my-app   [CRITICAL]    │
│                                         │
│  Summary                                │
│  Container crashed due to OOMKilled…    │
│                                         │
│  Root Cause                             │
│  Memory limit of 256Mi exceeded…        │
│                                         │
│  Insights                               │
│  • Container restarted 14 times         │
│  • Last exit code: 137 (OOMKilled)      │
│                                         │
│  Recommendations                        │
│  • Increase memory limit to 512Mi       │
│  • Add a Vertical Pod Autoscaler        │
│                                         │
│  Commands                               │
│  $ kubectl describe pod my-app -n…      │
│  $ kubectl top pod my-app -n default    │
│                                         │
│  [no AI key]  Set ANTHROPIC_API_KEY     │
│               to enable AI features     │
└─────────────────────────────────────────┘
```

### Analyze button placement

An **"Analyze"** button appears in:
1. The Pod Detail panel header (pod-specific analysis)
2. The title bar / toolbar (cluster-wide analysis)
3. Keyboard shortcut: `Ctrl+Shift+A` / `Cmd+Shift+A`

---

## Step 12 — `kairo-ui/src/app.rs` integration

### New `KubeEvent` variants

```rust
KubeEvent::AiAnalyzing(String),          // resource label — shows spinner
KubeEvent::AiResult(AiAnalysis),          // success
KubeEvent::AiError(String),              // failure message
```

### `Workspace` additions

```rust
pub struct Workspace {
    // ... existing fields ...
    ai_panel:     Entity<AiPanel>,
    ai_controller: Option<Arc<AiController>>,  // None if no API key
}
```

### Init in `Workspace::new`

```rust
let ai_controller = AiConfig::from_env().map(|cfg| Arc::new(AiController::new(cfg)));

// Right dock now has two tabs: YAML + AI Insights
let right = DockItem::tabs(
    vec![
        Arc::new(yaml_panel.clone())  as Arc<dyn PanelView>,
        Arc::new(ai_panel.clone())    as Arc<dyn PanelView>,
    ],
    &weak_dock, window, cx,
);
```

Note: the tab-switching problem from Phase 12 doesn't apply here because:
- YAML opens right dock when a resource is selected (user intent = see YAML)
- AI opens right dock when user explicitly clicks Analyze (user intent = see AI)
- The tabs are for browsing, not auto-switched programmatically

### `analyze_pod` trigger

```rust
fn on_analyze_pod(&mut self, cx: &mut Context<Self>) {
    let Some(ai) = self.ai_controller.clone() else { return };
    let Some(detail) = self.current_pod_detail.clone() else { return };
    let events = self.events.clone();
    let label = format!("Pod / {}/{}", detail.summary.namespace, detail.summary.name);

    // Show spinner immediately
    self.ai_panel.update(cx, |panel, cx| panel.set_loading(label.clone(), cx));

    // Fetch log tail + run analysis concurrently
    let client = self.kube_client.clone();
    kube_runtime::handle().spawn(async move {
        let log_tail = if let Some(c) = client {
            c.fetch_pod_logs_tail(&detail.summary.namespace, &detail.summary.name, None, 50)
                .await.ok()
        } else { None };

        match ai.analyze_pod(&detail, log_tail, &[]).await {
            Ok(analysis) => events.lock().unwrap().push_back(KubeEvent::AiResult(analysis)),
            Err(e) => events.lock().unwrap().push_back(KubeEvent::AiError(e.to_string())),
        }
    });
}
```

---

## New dependency summary

| Crate | Version | Where | Reason |
|-------|---------|-------|--------|
| `reqwest` | `0.12` | `kairo-ai` | HTTP client for Anthropic API |
| `kairo-ai` | local path | `kairo-ui` | AI engine |

No other new deps. `serde`, `serde_json`, `tokio`, `tracing`, `thiserror` are all
already approved and used in `kairo-core`.

---

## API Key handling

| Scenario | Behavior |
|----------|----------|
| `ANTHROPIC_API_KEY` set | AI features fully enabled |
| Key not set | `AiController` is `None`; Analyze button is greyed out |
| Key invalid / network error | `KubeEvent::AiError` → `AiPanel` shows error message |
| No pod selected | Analyze button disabled; cluster analysis still available |

The key is **never stored to disk**. It is read at startup from the environment and
held in memory only.

---

## Prompt caching economics

The system prompt is ~400 tokens. With `cache_control: ephemeral`:
- First call: full pricing (input tokens)
- Subsequent calls within 5 min: ~10% of input token cost for the system turn

For a session with 20 analysis calls, cached calls save ~90% on the system prompt cost.

---

## Verification

```bash
cargo check                          # all four crates compile
cargo clippy -- -D warnings          # clean

# With ANTHROPIC_API_KEY set + live cluster:
cargo run -p kairo-ui
# 1. Select a CrashLoopBackOff pod
# 2. Click "Analyze" in pod detail header
# 3. Right dock / AI Insights tab shows result within ~3s
```

---

## What this phase does NOT include

- No model selector UI (env var `KAIRO_AI_MODEL` overrides, defaults to sonnet-4-6)
- No conversation / follow-up questions (single-turn analysis only)
- No analysis history / persistence
- No streaming response rendering (full response rendered on completion)

All of the above are good candidates for a follow-up phase.

---

## Summary table

| Step | File | Change |
|------|------|--------|
| 1 | `Cargo.toml` | Add `kairo-ai` workspace member |
| 2 | `crates/kairo-ai/Cargo.toml` | New crate, `reqwest` dep |
| 3 | `kairo-core/src/models.rs` | `Serialize` on all summary structs |
| 4 | `kairo-core/src/client.rs` | `fetch_pod_logs_tail` |
| 5 | `kairo-ai/src/error.rs` | `AiError` |
| 6 | `kairo-ai/src/config.rs` | `AiConfig::from_env()` |
| 7 | `kairo-ai/src/context.rs` | `ContextBuilder`, `AnalysisContext` variants |
| 8 | `kairo-ai/src/prompts.rs` | `SYSTEM_PROMPT` constant + `PromptEngine` |
| 9 | `kairo-ai/src/provider.rs` | `LlmProvider` with prompt caching |
| 10 | `kairo-ai/src/response.rs` | `AiAnalysis`, `Severity`, `ResponseParser` |
| 11 | `kairo-ai/src/controller.rs` | `AiController` orchestrator |
| 12 | `kairo-ai/src/lib.rs` | Public re-exports |
| 13 | `kairo-ui/src/components/ai_panel.rs` | `AiPanel` dock component |
| 14 | `kairo-ui/src/components/mod.rs` | Register `ai_panel` |
| 15 | `kairo-ui/src/app.rs` | Wire controller + events + Analyze trigger |
