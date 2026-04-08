# Key Facts

Project constants and configuration reference. **Never store secrets here** — only non-sensitive identifiers and references.

---

### Workspace Structure

- Workspace root: `kubescope/` (two crates)
- Core crate: `crates/kairo-core/` — K8s logic, zero UI deps
- UI crate: `crates/kairo-ui/` — GPUI application

### Approved Dependency Versions

**Core (`kairo-core`):**
- `kube` = `3.1.0` (features: `runtime, client, rustls-tls`)
- `k8s-openapi` = `0.27.0` (features: `latest`)
- `tokio` = `1` (features: `full`)

**UI (`kairo-ui`):**
- `gpui` — git dep: `https://github.com/zed-industries/zed`
- `gpui-component` — git dep: `https://github.com/longbridge/gpui-component`
- Pin both to the same compatible rev when builds break

### Key Commands

```bash
cargo check                                         # verify compilation
cargo clippy -- -D warnings                         # lint (warnings = errors)
cargo test -p kairo-core                        # unit tests
cargo test -p kairo-core --features integration # needs live cluster
cargo run -p kairo-ui                           # launch the app
```

### Important URLs

- GPUI README: https://github.com/zed-industries/zed/tree/main/crates/gpui
- GPUI examples: https://github.com/zed-industries/zed/tree/main/crates/gpui/examples
- gpui-component docs: https://longbridge.github.io/gpui-component/
- gpui-component LLM docs: https://longbridge.github.io/gpui-component/llms.txt
- gpui-component stories: https://github.com/longbridge/gpui-component/tree/main/crates/story
- kube-rs docs: https://kube.rs/
- kube-rs examples: https://github.com/kube-rs/kube/tree/main/examples

### Local Development

- kubeconfig: standard `~/.kube/config` (kube-rs reads it automatically)
- No custom ports or proxies required for local development
- macOS build prereqs: Xcode + macOS components + command line tools

### Phase Progress

Current phase: **Phase 2 completed** (core client & models with `From<Pod>` conversions)
Next: **Phase 3** — watchers & logs
