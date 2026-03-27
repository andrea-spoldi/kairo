# KubeScope

A native Rust desktop Kubernetes IDE (think Lens, but without Electron).
Built with GPUI + kube-rs. Targets macOS and Linux.

## GPUI Dependency Strategy

GPUI lives inside the Zed monorepo (`zed-industries/zed/crates/gpui`) and is
**not published as a standalone crate** on crates.io. The `gpui` name on crates.io
is a stale snapshot (0.2.2). A community fork `gpui-ce` exists on crates.io (0.3)
but is 381+ commits behind upstream and **incompatible with gpui-component**.

We use **git dependencies pointing to the Zed monorepo** for GPUI, and git deps
for gpui-component from Longbridge. This is the setup Longbridge themselves
recommend and the only path that gives us the 60+ component library.

**Critical**: both `gpui` and `gpui-component` must be pinned to compatible git
revisions. If builds break after a GPUI update, pin `gpui-component` first (it
tracks upstream GPUI via dependabot), then match the GPUI rev it expects.

Fallback plan: if the git dep situation becomes unmanageable, we can drop
`gpui-component` and switch to `gpui-ce` from crates.io, building UI components
from raw GPUI primitives. The `kubescope-core` crate is unaffected by this
choice since it has zero UI dependencies.

## Architecture

Two-crate workspace. The K8s layer has **zero** UI dependencies.

```
kubescope/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── kubescope-core/         # K8s client, watchers, models (NO gpui imports)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs       # kubeconfig loading, context switching
│   │       ├── watchers.rs     # kube::runtime::watcher-based streams
│   │       ├── models.rs       # PodSummary, PodDetail, ContainerStatus, PodEvent
│   │       └── logs.rs         # log follow stream abstraction
│   └── kubescope-ui/           # GPUI application
│       └── src/
│           ├── main.rs
│           ├── app.rs          # root state, dock layout shell
│           ├── theme.rs        # colors, spacing constants
│           ├── actions.rs      # GPUI keyboard actions
│           └── components/
│               ├── mod.rs
│               ├── context_switcher.rs
│               ├── namespace_selector.rs
│               ├── pod_list.rs
│               ├── pod_detail.rs
│               ├── log_viewer.rs
│               └── search_bar.rs
```

### Hard rules

- `kubescope-core` must NEVER import `gpui`. If you feel the urge, you’re putting UI logic in the wrong crate.
- All K8s API interaction goes through `kubescope-core`. The UI crate calls core functions, never constructs `Api<T>` directly.
- Pod list updates use `kube::runtime::watcher` (event-driven). No polling loops.
- No `.unwrap()` in non-test code. Propagate errors with `?` or handle explicitly.

## Dependencies

Approved dependency list. Do NOT add others without asking.

**UI crate (`kubescope-ui`):**

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
gpui-component-assets = { git = "https://github.com/longbridge/gpui-component" }
```

If builds break, pin both to a specific rev:

```toml
gpui = { git = "https://github.com/zed-industries/zed", rev = "<PINNED_REV>" }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "<PINNED_REV>" }
```

**Core crate (`kubescope-core`):**

|Crate                                |Version |Notes                                  |
|-------------------------------------|--------|---------------------------------------|
|`kube`                               |`3.1.0` |features: `runtime, client, rustls-tls`|
|`k8s-openapi`                        |`0.27.0`|features: `latest`                     |
|`tokio`                              |`1`     |features: `full`                       |
|`futures`                            |latest  |stream combinators                     |
|`tracing`                            |latest  |structured logging                     |
|`thiserror`                          |latest  |library error types                    |
|`serde` + `serde_json` + `serde_yaml`|latest  |serialization                          |
|`chrono`                             |latest  |age calculations                       |

**Shared / app-level:**

|Crate               |Version|Notes                                   |
|--------------------|-------|----------------------------------------|
|`anyhow`            |latest |app-level error handling (ui crate only)|
|`tracing-subscriber`|latest |log output formatting                   |

## Commands

```bash
cargo check                           # verify compilation
cargo clippy -- -D warnings           # lint, treat warnings as errors
cargo test -p kubescope-core          # unit tests
cargo test -p kubescope-core --features integration  # needs a live cluster
cargo run -p kubescope-ui             # launch the app
```

## Code Style

- Rust 2021 edition
- Doc comments (`///`) on all public types and functions in `kubescope-core`
- `#[derive(Debug, Clone)]` on all data model structs
- Error types use `thiserror` with human-readable messages
- Prefer `impl Into<SharedString>` for GPUI string params
- snake_case functions, CamelCase types, SCREAMING_SNAKE_CASE constants
- Keep functions under 50 lines; extract if longer

## Git

- Init repo on first scaffold
- Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`
- Commit after each completed phase, not after every file

## Phase Plan

Work through phases in order. Each phase must compile and pass `cargo clippy` before moving on.

### Phase 1 — Scaffold (start here)

Create the workspace, all `Cargo.toml` files, and stub modules.
`main.rs` opens an empty GPUI window with a title. Everything compiles.

**Critical first step**: verify that the gpui + gpui-component git deps resolve
and compile together. If they don’t, find a compatible rev pair before proceeding.

Use `gpui_component::init(cx)` in the app entry point and wrap the root view in
a `gpui_component::Root`. Refer to the gpui-component getting started guide:
https://longbridge.github.io/gpui-component/docs/getting-started

**Done when:** `cargo check` succeeds and `cargo run -p kubescope-ui` opens a window.

### Phase 2 — Core: Client & Models

Implement in `kubescope-core`:

- `client.rs`: load kubeconfig, list contexts, create a client for a given context
- `models.rs`: `PodSummary` (name, namespace, status, ready count, restarts, age, node),
  `PodDetail`, `ContainerStatus`, `PodEvent` — with `From<Pod>` conversions
- Unit tests for model conversions (use fixture JSON)

**Done when:** `cargo test -p kubescope-core` passes with model conversion tests.

### Phase 3 — Core: Watchers & Logs

Implement in `kubescope-core`:

- `watchers.rs`: pod watcher using `kube::runtime::watcher` that sends `PodSummary`
  updates through a `tokio::sync::mpsc` channel. Namespace watcher for the namespace list.
- `logs.rs`: async log stream for a specific pod/container using the K8s log follow API

**Done when:** an integration example connects to a live cluster and prints pod events to stdout.

### Phase 4 — UI: App Shell

Build the GPUI app shell using gpui-component’s Dock layout:

- DockArea with: left sidebar (resource tree placeholder), main panel (pod list),
  right panel (detail, hidden by default), bottom panel (logs, hidden by default)
- TitleBar with app name and placeholder dropdowns
- Theme constants: dark background, status colors (green/yellow/red/grey)

Reference the gpui-component DockArea docs and story examples.

**Done when:** `cargo run -p kubescope-ui` shows the dock layout with placeholder content.

### Phase 5 — UI: Context & Namespace Selectors

Wire real data:

- Context switcher dropdown in title bar, populated from kubeconfig
- Namespace selector dropdown, populated by namespace watcher
- Switching context reinitializes the kube client and restarts watchers
- Switching namespace filters the pod list

**Done when:** dropdowns show real contexts/namespaces from the local kubeconfig.

### Phase 6 — UI: Pod List

- Virtualized table using gpui-component’s Table component
- Columns: Name, Namespace, Status (colored dot), Ready, Restarts, Age, Node
- Real-time updates from the pod watcher channel
- Status colors: Running=green, Pending=yellow, Failed/CrashLoopBackOff=red, Succeeded=grey

**Done when:** pod list shows real pods and updates live (test by scaling a deployment).

### Phase 7 — UI: Pod Detail Panel

- Clicking a pod row opens/updates the right dock panel
- Sections: Metadata (labels, annotations), Conditions, Container Statuses, Events
- Events fetched via field selector for the selected pod

**Done when:** clicking a pod shows its detail; events load correctly.

### Phase 8 — UI: Log Viewer

- Bottom dock panel streams logs from the selected pod’s first container
- Auto-scroll to bottom with pause/resume toggle
- Container selector if pod has multiple containers

**Done when:** log panel streams real logs with follow behavior.

### Phase 9 — UI: Search & Filter

- Search bar above the pod list (gpui-component Input)
- Filter by: name substring, label selector (key=value), status dropdown
- Filters apply client-side on the cached pod list

**Done when:** filtering works and updates the list in real-time.

## Stretch Goals (after Phase 9 is solid)

- Vim-style `j/k` navigation in pod list, `/` to focus search
- Resource type selector (Deployments, Services, ConfigMaps, etc.)
- Raw YAML viewer with syntax highlighting (gpui-component Editor)
- Node overview with resource allocation bars
- Multi-cluster tabs

## Key References

- GPUI README: https://github.com/zed-industries/zed/tree/main/crates/gpui
- GPUI examples: https://github.com/zed-industries/zed/tree/main/crates/gpui/examples
- gpui-component docs: https://longbridge.github.io/gpui-component/
- gpui-component LLM docs: https://longbridge.github.io/gpui-component/llms.txt
- gpui-component stories (examples): https://github.com/longbridge/gpui-component/tree/main/crates/story
- kube-rs docs: https://kube.rs/
- kube-rs examples: https://github.com/kube-rs/kube/tree/main/examples

## Troubleshooting

- **GPUI compile errors on Linux**: needs `libxkbcommon-dev`, `libwayland-dev`, Vulkan SDK, and `cmake`
- **GPUI compile errors on macOS**: needs Xcode with macOS components + command line tools
- **gpui + gpui-component version mismatch**: check gpui-component’s Cargo.toml for the gpui
  git rev it expects, then align your workspace’s gpui dep to match
- **kube-rs can’t connect**: verify `kubectl cluster-info` works; kube-rs reads the same kubeconfig
- **Slow first build**: the Zed monorepo git dep pulls a lot; subsequent builds use cargo cache
