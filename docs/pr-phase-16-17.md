## AI Agent Panel · Settings · MCP · Event Analysis

This PR adds the AI assistant layer to Kairo: a persistent settings panel,
a streaming chat agent in the right dock, one-click event analysis from any
warning card, curated SRE prompts, and rich structured rendering of analysis
results.

---

### Settings Panel (`⚙` / `Ctrl+,` / `Cmd+,`)

- Modal overlay with four tabs: **Anthropic**, **OpenAI**, **Ollama**, **MCP**
- Per-provider fields: API key, model, base URL, max tokens
- "Set active" toggle — one provider active at a time, shown with a `●` indicator
- Persisted to `~/.kairo/config.toml` with `0600` Unix permissions (API keys safe from other users)
- New **`kairo-config`** crate owns all persistence; independently testable with `#[serde(default)]` round-trips

### AI Agent Panel (right dock)

- Streaming chat backed by **Anthropic**, **OpenAI-compatible endpoints**, or **Ollama**
- Context header shows the currently selected K8s resource (pod, deployment, …)
- `ChatEntry.api_content` field decouples the short display bubble from the full prompt sent to the LLM
- History capped at 100 messages with automatic front-trim
- Streaming cursor `▊`, Clear button, Enter-to-send

### MCP Server Configuration

- New **MCP** tab in Settings: server URL + Enable/Disable toggle
- Persisted as `[mcp]` in `~/.kairo/config.toml`
- When enabled, the MCP server URL appears in the system prompt context
- Lays groundwork for live cluster tool-calling via the [Model Context Protocol](https://modelcontextprotocol.io)

### `⬡ Analyze` buttons on event cards

- Every **Warning Event** card in the Events feed has an Analyze badge
- Every **event row** inside the Pod Detail panel has an Analyze badge
- Clicking serialises the event to a JSON context block (reason, message, type,
  count, timestamps, namespace/resource) and sends a structured SRE analysis
  prompt directly to the AI panel — no copy-paste required

### Curated SRE prompts

| Prompt | Content |
|--------|---------|
| **System** | Expert Kubernetes SRE persona: confidence levels, multiple hypotheses, no hallucination, actionable-first |
| **User (event analysis)** | Structured `<JSON>` context block asking for `{"hypotheses": [...], "next_steps": [...]}` output |

Regular free-form chat continues to work with the enriched system prompt.

### Rich analysis rendering

Instead of displaying raw JSON, completed analysis responses are parsed and rendered as UI cards:

```
┌─ AI ────────────────────────────────────────────────────┐
│                                                         │
│  ┌─ HIGH ──────────────────────────────────────────┐    │
│  │  Container image pull failure                   │    │
│  │    • ImagePullBackOff event (14×)               │    │
│  │    • No successful pull in last 5 minutes       │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
│  ┌─ MEDIUM ────────────────────────────────────────┐    │
│  │  Registry credentials expired                  │    │
│  │    • Last successful pull 48 hours ago          │    │
│  └─────────────────────────────────────────────────┘    │
│                                                         │
│  Next Steps                                             │
│    1. Check image name and tag in the deployment spec   │
│    2. Verify imagePullSecrets exist in the namespace    │
│    3. Run: kubectl describe pod <name> -n <ns>          │
└─────────────────────────────────────────────────────────┘
```

Confidence badges: `HIGH` = green · `MEDIUM` = amber · `LOW` = red.
Falls back to plain text for regular chat messages or when JSON parsing fails.

---

### Architecture notes

- `kairo-config` — new crate, zero UI/core deps, `serde` + `toml` only
- `analyze.rs` — shared `AnalyzeEventRequest(String)` event type lets both
  `event_feed` and `pod_detail` emit to `app.rs` without coupling
- `ChatEntry.api_content: Option<String>` — display text and API payload can differ
- `ClusterEvent` and `PodEvent` now derive `serde::Serialize` for JSON event context

### Test / quality

```
cargo test -p kairo-config   # 3 round-trip tests — ok
cargo clippy -- -D warnings  # clean
cargo check                  # clean
```

### Files changed (summary)

| File | Change |
|------|--------|
| `crates/kairo-config/` | New crate: `McpConfig`, `AiConfig`, `KairoConfig` with TOML persistence |
| `crates/kairo-ui/src/ai_client.rs` | `SYSTEM_PROMPT`, `build_event_analysis_prompt()`, streaming providers |
| `crates/kairo-ui/src/components/ai_panel.rs` | Streaming chat panel, `push_analysis_message`, rich JSON renderer |
| `crates/kairo-ui/src/components/settings_panel.rs` | Four-tab settings modal |
| `crates/kairo-ui/src/components/event_feed.rs` | `⬡ Analyze` badge on every event card |
| `crates/kairo-ui/src/components/pod_detail.rs` | `⬡ Analyze` badge on every pod event row |
| `crates/kairo-ui/src/analyze.rs` | Shared `AnalyzeEventRequest` event type |
| `crates/kairo-ui/src/app.rs` | Right dock wiring, `handle_analyze_event`, updated system prompt |
| `crates/kairo-core/src/models.rs` | `#[derive(Serialize)]` on `ClusterEvent`, `PodEvent` |
