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
│  Kairo   [cluster ▾]  [namespace ▾]                            ⚙            │
├─ Cluster Health ──────┬─ Pods / Events ────────────────┬─ AI Agent ─────── ┤
│  ● 47  Running        │  NAME          STATUS  RESTARTS │ ⬡ prod/worker-..  │
│  ◐  2  Pending        │  api-server    ● Run        0   │                   │
│  ✖  1  Failed         │  worker-65f9b  ✖ Crash     14   │ You               │
│  ─────────────        ├─ Pod Detail ───────────────────┤ Analyze event:    │
│  NAMESPACES           │  worker-65f9b · CrashLoopBackOff│ BackOff           │
│  production   ✖1      │  Events                         │                   │
│  staging      ●8      │  ✖ BackOff ×14  [⬡ Analyze]    │ AI                │
│  ─────────────        ├─ Logs ─────────────────────────┤ ┌─ HIGH ────────┐ │
│  RECENT WARNINGS      │  Error: connection refused      │ │ Image pull    │ │
│  ⚠ BackOff  [⬡]      │                                 │ │ • BackOff ×14 │ │
│                       │                                 │ └───────────────┘ │
│                       │                                 │ Next Steps        │
│                       │                                 │  1. Check image   │
│                       │                                 │  2. Verify secret │
├───────────────────────┴─────────────────────────────────┴───────────────────┤
│  ● minikube  │  production  ·  ● 47  ◐ 2  ✖ 1  · 50 pods                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## What's new in v0.12.0

- **Dark glass theme** — redesigned left dock and panel chrome with a translucent glass aesthetic
- **Kind-filter checkboxes** — filter the resource tree by resource type (Pods, Deployments, Services, …) without leaving the panel
- **Namespace filter in resource tree** — scoped directly from the left dock dropdown
- **AI status moved into Agent panel** — cleaner top bar; provider status is shown inline inside the panel
- **Details panel fixes** — restored scrolling, prevented content overflow in section cards
- **Removed StatsPanel** — cluster stats are now consolidated in the Details panel

## What's new in v0.11.0

- **Hierarchical resource tree** — Deployments, StatefulSets, and DaemonSets show their owned pods as children
- **AgentScope state machine** — the AI agent's context scope (cluster / namespace / resource) follows your selection automatically
- **Investigation flows** — three built-in prompt flows: *Crash Loop*, *Image Pull*, and *OOMKilled* — one click sends a structured SRE investigation prompt pre-loaded with the relevant resource context

## Features

### Situational awareness
- **Cluster health sidebar** — pod counts by status (●/◐/✖), broken down per namespace
- **Live warning event feed** — cluster-wide `Warning` events surfaced in real time, newest first
- **Status bar** — always-visible cluster name, active namespace, and pod health summary

### Resource tree
- Hierarchical view: Deployments/StatefulSets/DaemonSets → owned Pods
- Covers: Pods, Deployments, Services, ConfigMaps, Nodes
- **Kind-filter checkboxes** to show only the resource types you care about
- **Namespace filter** directly in the resource tree dropdown

### Pod list
- Real-time updates via Kubernetes watchers — no polling
- Filter by name, label selector (`app=nginx`), and status
- **Controller grouping** — group pods under their owning Deployment, StatefulSet, or DaemonSet
- Keyboard navigation: `j`/`k` to move, `/` to filter, `Enter` to open detail, `g` to toggle grouping

### Pod detail & logs
- Metadata, labels, annotations, conditions, and container statuses
- Related events fetched per pod
- **Live log streaming** with container selector and auto-scroll

### AI Agent (right dock)
- Streaming chat backed by **Anthropic**, **OpenAI-compatible endpoints**, or **Ollama**
- **AgentScope** — context scope (cluster / namespace / resource) follows your selection automatically
- **Investigation flows** — one-click SRE prompts for *Crash Loop*, *Image Pull*, and *OOMKilled*
- **`⬡ Analyze` button** on every warning event card and every pod event row — one click sends the event as a structured SRE analysis prompt
- Analysis responses rendered as visual cards: color-coded confidence badges (`HIGH` / `MEDIUM` / `LOW`), evidence bullets, and numbered next steps
- MCP server support: connect a [Kubernetes MCP server](https://modelcontextprotocol.io) to give the agent live cluster tool access

### Settings (`⚙` or `Ctrl+,` / `Cmd+,`)
- Per-provider configuration: Anthropic, OpenAI-compatible, Ollama
- API keys persisted to `~/.kairo/config.toml` with `0600` Unix permissions
- MCP server URL + enable/disable toggle

### Native & fast
- Built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) — Zed's GPU-accelerated UI framework, no Electron
- Dark glass theme, WCAG AA contrast throughout
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

## Setup

### 1. Kubernetes access

Kairo reads your kubeconfig automatically from `~/.kube/config` (the same file `kubectl` uses).
No additional configuration is required — if `kubectl cluster-info` works, Kairo will connect.

To use a non-default kubeconfig:

```bash
KUBECONFIG=/path/to/your/config kairo
```

Minimum RBAC required (read-only):

```yaml
rules:
  - apiGroups: [""]
    resources: ["pods", "namespaces", "events", "pods/log", "nodes", "services", "configmaps"]
    verbs: ["get", "list", "watch"]
  - apiGroups: ["apps"]
    resources: ["deployments", "statefulsets", "daemonsets", "replicasets"]
    verbs: ["get", "list", "watch"]
```

### 2. AI Agent (optional)

Open **Settings** (`⚙` in the top bar, or `Cmd+,` / `Ctrl+,`) and configure one provider:

| Provider | What to fill in |
|---|---|
| **Anthropic** | API key from [console.anthropic.com](https://console.anthropic.com) |
| **OpenAI-compatible** | Base URL + API key (works with OpenAI, Azure OpenAI, LM Studio, etc.) |
| **Ollama** | Base URL of your local Ollama server (default: `http://localhost:11434`) |

Config is saved to `~/.kairo/config.toml` with `0600` permissions. The file is created on first save.

### 3. MCP server (optional)

If you have a [Kubernetes MCP server](https://modelcontextprotocol.io) running, enter its URL in the **MCP** tab of Settings and enable it. This allows the AI agent to query the cluster directly during a conversation.

## Building from source

### Prerequisites

**macOS**
```bash
xcode-select --install   # Xcode command line tools (includes Metal + clang)
```

> Xcode must include macOS platform components. Open Xcode → Settings → Platforms and install **macOS** if it is not already listed.

**Linux (Ubuntu / Debian)**
```bash
sudo apt-get install -y \
  cmake pkg-config \
  libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libvulkan-dev \
  libx11-dev libxcb1-dev libxcb-xkb-dev
```

**Rust toolchain** (both platforms)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Kairo requires a recent stable Rust (1.80+). `rustup update stable` if you are on an older version.

### Build

```bash
git clone https://github.com/andrea-spoldi/kubescope_
cd kubescope_

# Required: recreate the vendored gpui-component (gitignored, see note below)
bash scripts/setup-vendor.sh

cargo build --release -p kairo-ui
./target/release/kairo
```

> **Vendor note:** Kairo applies a source-level patch to `gpui-component` to
> suppress a cosmetic UI issue (greyed "Zoom In" in every panel menu).
> Because the patched source is too large to commit, `vendor/` is gitignored
> and must be recreated with `setup-vendor.sh` after every fresh clone.
> The script clones `gpui-component` at the exact pinned rev, adjusts its
> Cargo.toml, and applies the patch automatically.

> **First build note:** GPUI is pulled from the Zed monorepo via git.
> The initial fetch can take a few minutes; subsequent builds use Cargo's
> local cache.

### Useful commands

```bash
cargo check                           # fast compilation check
cargo clippy -- -D warnings           # lint (warnings are errors)
cargo test -p kairo-core              # unit tests (no cluster needed)
cargo test -p kairo-config            # config round-trip tests
cargo run -p kairo-ui                 # run in development mode
bash scripts/make-icons.sh            # PNG → .icns (macOS)
bash scripts/make-dmg.sh              # build drag-to-install DMG (macOS)
```

## Philosophy

**Kairo is read-only by design.**

Mutations belong in your GitOps pipeline. Kairo's job is to show you
the truth about what the cluster is actually doing, clearly and immediately,
so you can make the right call at the right moment.

## Architecture

```
kairo/
├── crates/
│   ├── kairo-config/   # Persistent config — zero UI/core deps
│   │   └── src/
│   │       ├── ai.rs   # AiConfig, provider structs, ActiveProvider
│   │       └── mcp.rs  # McpConfig — MCP server URL + enabled flag
│   ├── kairo-core/     # K8s client, watchers, models — zero UI deps
│   │   └── src/
│   │       ├── client.rs   # kubeconfig loading, context switching
│   │       ├── watchers.rs # event-driven pod/namespace/event watchers
│   │       ├── models.rs   # PodSummary, PodDetail, ClusterEvent, …
│   │       └── logs.rs     # log follow stream
│   └── kairo-ui/       # GPUI application
│       └── src/
│           ├── app.rs               # root workspace, event bridge
│           ├── ai_client.rs         # streaming LLM client (Anthropic/OpenAI/Ollama)
│           ├── analyze.rs           # shared AnalyzeEventRequest event type
│           ├── theme.rs             # semantic colour constants
│           └── components/
│               ├── ai_panel.rs        # streaming chat + rich analysis renderer
│               ├── cluster_health.rs  # left sidebar
│               ├── event_feed.rs      # warning events tab + Analyze buttons
│               ├── pod_list.rs        # main pod table
│               ├── pod_detail.rs      # detail panel + Analyze buttons on events
│               ├── log_viewer.rs      # bottom log panel
│               └── settings_panel.rs  # settings modal (Anthropic/OpenAI/Ollama/MCP)
└── scripts/
    ├── make-icons.sh   # PNG → .icns pipeline (macOS)
    └── make-dmg.sh     # .app bundle + DMG packaging (macOS)
```

The K8s layer (`kairo-core`) has **zero UI dependencies** and is independently
testable. The UI crate calls core functions; it never constructs `Api<T>` directly.

## Roadmap

- [x] Command palette (`⌘K` / `Ctrl+K`) — jump to any pod, switch context/namespace
- [x] Hierarchical resource tree — Deployments, Services, ConfigMaps, Nodes with kind-filter checkboxes
- [x] Raw YAML viewer with syntax highlighting
- [x] Node overview with CPU/memory allocation bars
- [x] Settings panel — LLM provider configuration persisted to `~/.kairo/config.toml`
- [x] AI Agent panel — streaming chat with context-aware SRE prompts
- [x] AgentScope state machine — agent context follows your selection automatically
- [x] Investigation flows — Crash Loop, Image Pull, OOMKilled one-click prompts
- [x] Event analysis — `⬡ Analyze` button on every warning event card
- [x] MCP server integration settings — groundwork for live cluster tool-calling
- [ ] Live MCP tool execution — let the agent call read-only cluster operations
- [ ] Multi-cluster tabs

## License

MIT © Andrea Spoldi
