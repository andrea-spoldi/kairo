# Kairo

> **Act at the right moment in your Kubernetes cluster.**

Kairo is a real-time situational awareness UI for Kubernetes,
built in Rust with a GPU-accelerated interface.

It helps engineers understand what is happening in their cluster
*right now* — and what actually matters.

---

*From Greek **Καιρός** (Kairos) — the right moment, the opportune instant, as opposed to
Chronos, which is clock time. The ancient Greeks understood that timing is everything.
So does your on-call engineer at 2 a.m.*

---

## What Kairo is

Kairo is a **read-only observability tool**. It does not create, patch, or delete resources.
Instead it watches the cluster continuously and surfaces what demands attention —
failed pods, warning events, restart storms — before you have to go hunting for them.

Think of it as **k9s ergonomics inside a native desktop window**: keyboard-driven,
GPU-rendered, always up to date, with no browser and no cloud dependency.

```
┌─ Title bar ─────────────────────────────────────────────────────────────────┐
│  Kairo   [cluster ▾]  [namespace ▾]                                         │
├─ Cluster Health ──────┬─ Pods / Events ──────────────────────────────────── ┤
│  ● 47  Running        │  NAME               STATUS    READY  RESTARTS  AGE  │
│  ◐  2  Pending        │  api-server-x4k2p   ● Running  1/1        0   12d   │
│  ✖  1  Failed         │  worker-65f9b       ✖ CrashLoop 0/1       14    3m  │
│  ─────────────        │  ...                                                │
│  NAMESPACES           ├─ Pod Detail ─────────────────────────────────────── ┤
│  production   ✖1      │  worker-65f9b · CrashLoopBackOff                    │
│  staging      ●8      │  Containers · Events · Labels                       │
│  default      ●12     ├─ Logs ───────────────────────────────────────────── ┤
│  ─────────────        │  [app] Error: connection refused to postgres:5432    │
│  RECENT WARNINGS      │  [app] Backing off for 10s before retry…            │
│  ⚠ BackOff   2m ago   │                                                     │
│  ⚠ OOMKilled 5m ago   │                                                     │
├───────────────────────┴─────────────────────────────────────────────────────┤
│  ● minikube  │  production  ·  ● 47  ◐ 2  ✖ 1  · 50 pods                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Features

### Situational awareness
- **Cluster health sidebar** — pod counts by status (●/◐/✖), broken down per namespace
- **Live warning event feed** — cluster-wide `Warning` events surfaced in real time, newest first
- **Status bar** — always-visible cluster name, active namespace, and pod health summary

### Pod list
- Real-time updates via Kubernetes watchers — no polling
- Filter by name, label selector (`app=nginx`), and status
- **Controller grouping** — group pods under their owning Deployment, StatefulSet, or DaemonSet
- Keyboard navigation: `j`/`k` to move, `/` to filter, `Enter` to open detail, `g` to toggle grouping

### Pod detail & logs
- Metadata, labels, annotations, conditions, and container statuses
- Related events fetched per pod
- **Live log streaming** with container selector and auto-scroll

### Native & fast
- Built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) — Zed's GPU-accelerated UI framework, no Electron
- Dark theme, WCAG AA contrast throughout
- Targets macOS (Apple Silicon + Intel) and Linux (x86_64)

## Installation

### macOS
Download `Kairo-<version>-macos.dmg` from the [Releases](../../releases) page,
open it, and drag **Kairo.app** to Applications.

### Linux
Download the pre-built binary from the [Releases](../../releases) page:

```bash
tar -xzf kairo-<version>-x86_64-unknown-linux-gnu.tar.gz
sudo mv kairo /usr/local/bin/
```

## Building from source

### Prerequisites

**macOS**
```bash
xcode-select --install   # Xcode command line tools
```

**Linux (Ubuntu / Debian)**
```bash
sudo apt-get install -y \
  cmake pkg-config \
  libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libvulkan-dev \
  libx11-dev libxcb1-dev
```

### Build

```bash
git clone https://github.com/andrea-spoldi/kubescope_
cd kubescope_
cargo build --release -p kairo-ui
./target/release/kairo
```

### Useful commands

```bash
cargo check                           # fast compilation check
cargo clippy -- -D warnings           # lint
cargo test -p kairo-core              # unit tests (no cluster needed)
cargo run -p kairo-ui                 # run in development mode
bash scripts/make-icons.sh            # PNG → .icns (macOS)
bash scripts/make-dmg.sh              # build drag-to-install DMG (macOS)
```

## Philosophy

**Kairo is read-only by design.**

Mutations belong in your GitOps pipeline. Kairo's job is to show you
the truth about what the cluster is actually doing, clearly and immediately,
so you can make the right call at the right moment.

Minimum RBAC required:

```yaml
rules:
  - apiGroups: [""]
    resources: ["pods", "namespaces", "events", "pods/log"]
    verbs: ["get", "list", "watch"]
```

## Architecture

```
kairo/
├── crates/
│   ├── kairo-core/     # K8s client, watchers, models — zero UI deps
│   │   ├── client.rs   # kubeconfig loading, context switching
│   │   ├── watchers.rs # event-driven pod/namespace/event watchers
│   │   ├── models.rs   # PodSummary, PodDetail, ClusterEvent, …
│   │   └── logs.rs     # log follow stream
│   └── kairo-ui/       # GPUI application
│       └── src/
│           ├── app.rs               # root workspace, event bridge
│           ├── theme.rs             # semantic colour constants
│           └── components/
│               ├── cluster_health.rs  # left sidebar
│               ├── event_feed.rs      # warning events tab
│               ├── pod_list.rs        # main pod table
│               ├── pod_detail.rs      # right detail panel
│               └── log_viewer.rs      # bottom log panel
└── scripts/
    ├── make-icons.sh   # PNG → .icns pipeline (macOS)
    └── make-dmg.sh     # .app bundle + DMG packaging (macOS)
```

The K8s layer (`kairo-core`) has **zero UI dependencies** and is independently
testable. The UI crate calls core functions; it never constructs `Api<T>` directly.

## Roadmap

- [ ] Command palette (⌘K) — jump to any pod, switch context/namespace
- [ ] Resource tree — Deployments, Services, ConfigMaps, Nodes
- [ ] Raw YAML viewer with syntax highlighting
- [ ] Node overview with CPU/memory allocation bars
- [ ] AI-assisted diagnosis — MCP integration for "why is this pod failing?"
- [ ] Multi-cluster tabs

## License

MIT © Andrea Spoldi
