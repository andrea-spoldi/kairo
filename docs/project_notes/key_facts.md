# Key Facts

Project constants and configuration reference. **Never store secrets here** — only non-sensitive identifiers and references.

---

### Workspace Structure

- Workspace root: `kubescope_/` (three crates)
- Config crate: `crates/kairo-config/` — persistent config, zero UI/core deps
- Core crate: `crates/kairo-core/` — K8s logic, zero UI deps
- UI crate: `crates/kairo-ui/` — GPUI application

### Approved Dependency Versions

**Config (`kairo-config`):**
- `serde` + `toml` = `0.8` — TOML serialization
- `dirs` = `5` — home directory lookup
- `thiserror` + `tracing` — errors and logging

**Core (`kairo-core`):**
- `kube` = `3.1.0` (features: `runtime, client, rustls-tls`)
- `k8s-openapi` = `0.27.0` (features: `latest`)
- `tokio` = `1` (features: `full`)
- `serde` (for `Serialize` on model types)

**UI (`kairo-ui`):**
- `gpui` — git dep: `https://github.com/zed-industries/zed`
- `gpui-component` — git dep: `https://github.com/longbridge/gpui-component`
- `reqwest` = `0.12` (features: `json, stream, rustls-tls`) — LLM HTTP client
- `futures-util` = `0.3` — async stream combinators
- `serde` + `serde_json` — JSON serialization for AI prompts
- Pin gpui + gpui-component to the same compatible rev when builds break

### Key Commands

```bash
cargo check                           # verify compilation
cargo clippy -- -D warnings           # lint (warnings = errors)
cargo test -p kairo-config            # 3 round-trip tests
cargo test -p kairo-core              # unit tests
cargo run -p kairo-ui                 # launch the app
```

### Important URLs

- GPUI README: https://github.com/zed-industries/zed/tree/main/crates/gpui
- GPUI examples: https://github.com/zed-industries/zed/tree/main/crates/gpui/examples
- gpui-component docs: https://longbridge.github.io/gpui-component/
- gpui-component LLM docs: https://longbridge.github.io/gpui-component/llms.txt
- gpui-component stories: https://github.com/longbridge/gpui-component/tree/main/crates/story
- kube-rs docs: https://kube.rs/
- kube-rs examples: https://github.com/kube-rs/kube/tree/main/examples
- MCP spec: https://modelcontextprotocol.io

### Local Development

- kubeconfig: standard `~/.kube/config` (kube-rs reads it automatically)
- AI config: `~/.kairo/config.toml` (created on first Settings save, permissions `0600`)
- No custom ports or proxies required for local development
- macOS build prereqs: Xcode + macOS components + command line tools
- DOCK_VERSION = `5` — bump this when the dock layout changes to clear persisted state

### Phase Progress

- Phase 1: ✅ Workspace scaffold + GPUI window
- Phase 2: ✅ Core client & models
- Phase 3: ✅ Watchers & logs
- Phase 4: ✅ App shell (dock layout)
- Phase 5: ✅ Context & namespace selectors
- Phase 6: ✅ Pod list (virtualized table, real-time)
- Phase 7: ✅ Pod detail panel
- Phase 8: ✅ Log viewer (streaming)
- Phase 9: ✅ Search & filter
- Phase 10–15: ✅ Command palette, resource tree (Deployments/Services/ConfigMaps/Nodes), YAML viewer, node resource bars, cluster health sidebar, event feed
- Phase 16: ✅ Settings panel + `kairo-config` crate
- Phase 17: ✅ AI Agent panel + event analysis + MCP config + rich rendering

**Current status: Phase 17 complete. Next: live MCP tool execution (let agent call read-only cluster operations).**
