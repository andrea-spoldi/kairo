# Phase 16 — Settings Panel

## Vision

A persistent, extensible settings system backed by `~/.kairo/config.toml`.
The first concrete use is configuring LLM providers (Phase 15), but the
schema is designed to grow — font preferences, cluster defaults, keybindings,
etc. can all slot in later without touching this layer.

A gear icon in the title bar (⚙) opens an in-app settings sheet. Changes are
written to disk on save and applied to the running app immediately.

---

## Config file location

| Platform | Path |
|----------|------|
| Linux    | `~/.kairo/config.toml` |
| macOS    | `~/.kairo/config.toml` |

Using `~/.kairo/` (not `~/.config/kairo/`) keeps it in a predictable,
git-ignore-friendly location consistent with other K8s tooling (kubectl,
helm, k9s all use `~/.kube/`, `~/.helm/`, etc.).

The directory is created on first save if absent.

---

## Architecture

```
kairo-config  (new crate — zero UI, zero K8s deps)
     ↓ depends on
kairo-ai      (reads AiProviderConfig to build backends)
     ↓ depends on
kairo-ui      (settings panel reads/writes KairoConfig; propagates changes)
```

A dedicated `kairo-config` crate keeps persistence logic testable and
re-usable by `kairo-mcp` in Phase 14 (the MCP server also needs AI keys).

---

## Config schema — `~/.kairo/config.toml`

```toml
# Kairo configuration — generated automatically, safe to hand-edit.

[general]
# Reserved for future: theme override, font size, etc.

[ai]
# Which provider is active: "anthropic", "openai", "ollama", or "" (disabled).
active_provider = "anthropic"

[ai.anthropic]
api_key    = ""
model      = "claude-sonnet-4-6"
max_tokens = 1024

[ai.openai]
# Also works with any OpenAI-compatible endpoint (Groq, Together, Azure, etc.)
api_key    = ""
base_url   = "https://api.openai.com/v1"
model      = "gpt-4o"
max_tokens = 1024

[ai.ollama]
# Local Ollama — no API key needed.
base_url   = "http://localhost:11434"
model      = "llama3.2"
max_tokens = 2048
```

Stored as TOML for human readability and easy hand-editing.

**Security note:** API keys are stored in plaintext in the user's home
directory. This is the same approach used by `gh`, `fly`, and most CLI tools.
File permissions are set to `0600` on creation. macOS Keychain integration
is a stretch goal.

---

## Files to create or modify

| File | Action |
|------|--------|
| `Cargo.toml` | Add `kairo-config` workspace member |
| `crates/kairo-config/Cargo.toml` | New crate |
| `crates/kairo-config/src/lib.rs` | `KairoConfig` + load/save + defaults |
| `crates/kairo-config/src/ai.rs` | `AiConfig`, `AnthropicConfig`, `OpenAiConfig`, `OllamaConfig` |
| `crates/kairo-ui/Cargo.toml` | Add `kairo-config` dependency |
| `crates/kairo-ui/src/components/settings_panel.rs` | `SettingsPanel` GPUI component |
| `crates/kairo-ui/src/components/mod.rs` | Register `settings_panel` |
| `crates/kairo-ui/src/actions.rs` | Add `OpenSettings` action |
| `crates/kairo-ui/src/app.rs` | Gear icon in title bar + settings wiring |

---

## Step 1 — Workspace `Cargo.toml`

```toml
members = [
    "crates/kairo-config",   # ← add
    "crates/kairo-core",
    "crates/kairo-ui",
    "crates/kairo-mcp",
]
```

---

## Step 2 — `crates/kairo-config/Cargo.toml`

```toml
[package]
name = "kairo-config"
version.workspace = true
edition.workspace = true

[dependencies]
serde    = { version = "1", features = ["derive"] }
toml     = "0.8"
dirs     = "5"
thiserror = "1"
tracing  = "0.1"
```

Two new deps to approve:
- **`toml`** `0.8` — TOML serialization/deserialization (small, no deps)
- **`dirs`** `5` — cross-platform home/config directory lookup

---

## Step 3 — `crates/kairo-config/src/ai.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ActiveProvider {
    Anthropic,
    OpenAi,
    Ollama,
    #[serde(other)]
    None,
}

impl Default for ActiveProvider {
    fn default() -> Self { Self::None }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub active_provider: ActiveProvider,
    pub anthropic: AnthropicConfig,
    pub openai:    OpenAiConfig,
    pub ollama:    OllamaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AnthropicConfig {
    pub api_key:    String,
    pub model:      String,
    pub max_tokens: u32,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key:    String::new(),
            model:      "claude-sonnet-4-6".into(),
            max_tokens: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub api_key:    String,
    pub base_url:   String,
    pub model:      String,
    pub max_tokens: u32,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key:    String::new(),
            base_url:   "https://api.openai.com/v1".into(),
            model:      "gpt-4o".into(),
            max_tokens: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub base_url:   String,
    pub model:      String,
    pub max_tokens: u32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url:   "http://localhost:11434".into(),
            model:      "llama3.2".into(),
            max_tokens: 2048,
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            active_provider: ActiveProvider::None,
            anthropic: AnthropicConfig::default(),
            openai:    OpenAiConfig::default(),
            ollama:    OllamaConfig::default(),
        }
    }
}
```

---

## Step 4 — `crates/kairo-config/src/lib.rs`

```rust
pub mod ai;

use std::{fs, path::PathBuf};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

pub use ai::{ActiveProvider, AiConfig, AnthropicConfig, OllamaConfig, OpenAiConfig};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to determine home directory")]
    NoHomeDir,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Top-level configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KairoConfig {
    pub ai: AiConfig,
    // general: GeneralConfig,  ← reserved for Phase 17+
}

impl KairoConfig {
    /// Return the path to `~/.kairo/config.toml`.
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
        Ok(home.join(".kairo").join("config.toml"))
    }

    /// Load from disk. Returns `Default` if the file does not exist yet.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;
        if !path.exists() {
            info!("no config file at {path:?} — using defaults");
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)?;
        let cfg: Self = toml::from_str(&raw)?;
        info!("loaded config from {path:?}");
        Ok(cfg)
    }

    /// Persist to `~/.kairo/config.toml`, creating the directory if needed.
    /// Sets file permissions to 0600 on Unix.
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }

        let toml_str = toml::to_string_pretty(self)?;
        fs::write(&path, &toml_str)?;

        // Restrict permissions on Unix (API keys live here).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }

        info!("saved config to {path:?}");
        Ok(())
    }
}
```

---

## Step 5 — `crates/kairo-ui/src/actions.rs`

Add alongside the existing `OpenCommandPalette`:

```rust
#[derive(Clone, PartialEq, Eq, Hash, Debug, gpui::Action)]
pub struct OpenSettings;
```

---

## Step 6 — `crates/kairo-ui/src/components/settings_panel.rs`

### Panel state

```rust
use kairo_config::{ActiveProvider, KairoConfig};
use gpui_component::input::{Input, InputState};

pub struct SettingsPanel {
    config:  KairoConfig,
    // One InputState per text field that needs live editing.
    anthropic_key:   Entity<InputState>,
    anthropic_model: Entity<InputState>,
    openai_key:      Entity<InputState>,
    openai_base_url: Entity<InputState>,
    openai_model:    Entity<InputState>,
    ollama_base_url: Entity<InputState>,
    ollama_model:    Entity<InputState>,
    active_tab:      ProviderTab,
    save_status:     SaveStatus,
}

#[derive(Clone, PartialEq)]
enum ProviderTab { Anthropic, OpenAi, Ollama }

#[derive(Clone)]
enum SaveStatus { Idle, Saved, Error(String) }
```

### Emitted event

```rust
/// Emitted after the user saves settings. Workspace reacts to reinitialise
/// any AI controller with the fresh config.
pub struct SettingsSaved(pub KairoConfig);
```

### Visual layout

```
┌─ Settings ──────────────────────────────────────────────────────────────┐
│                                                                          │
│  AI Integration                                                          │
│  ──────────────                                                          │
│  Provider   [ Anthropic ▼ ]  (or tab strip: Anthropic | OpenAI | Ollama)│
│                                                                          │
│  ┌─ Anthropic ────────────────────────────────────────────────────────┐ │
│  │  API Key    [sk-ant-••••••••••••••••••••••••••••••••••••••••••]    │ │
│  │  Model      [claude-sonnet-4-6                                ]    │ │
│  │  Max tokens [1024                                             ]    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ OpenAI ───────────────────────────────────────────────────────────┐ │
│  │  API Key    [sk-••••••••••••••••••••••••••••••••••••••••••••••]    │ │
│  │  Base URL   [https://api.openai.com/v1                        ]    │ │
│  │  Model      [gpt-4o                                           ]    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─ Ollama ───────────────────────────────────────────────────────────┐ │
│  │  Base URL   [http://localhost:11434                           ]    │ │
│  │  Model      [llama3.2                                         ]    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│                          [ Cancel ]   [ Save Settings ✓ ]               │
└──────────────────────────────────────────────────────────────────────────┘
```

The panel renders as a **floating centered modal** using gpui-component's
existing sheet layer (`Root::render_sheet_layer` is already wired in app.rs).

### Opening / closing

```rust
// In app.rs Render, add an action handler:
.on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
    this.settings_panel.update(cx, |panel, cx| {
        panel.show(window, cx);
    });
}))

// Gear button in title bar triggers the action:
.child(
    div()
        .cursor_pointer()
        .on_click(cx.listener(|_, _, cx| cx.dispatch_action(Box::new(OpenSettings))))
        .child(Label::new("⚙").text_sm().text_color(TEXT_MUTED))
)
```

Keyboard shortcut: `Cmd+,` (macOS) / `Ctrl+,` (Linux) — registered in `actions.rs`.

### Save flow

1. User edits fields and clicks **Save Settings**
2. Panel reads all `InputState` values back into a `KairoConfig`
3. Calls `config.save()` — writes `~/.kairo/config.toml`
4. Emits `SettingsSaved(config)` event
5. Panel shows ✓ confirmation for 2 seconds then returns to `Idle`

### Workspace reaction

```rust
cx.subscribe_in(&settings_panel, window,
    |this, _, event: &SettingsSaved, _w, cx| {
        this.on_settings_saved(event.0.clone(), cx);
    }
).detach();

fn on_settings_saved(&mut self, config: KairoConfig, cx: &mut Context<Self>) {
    // Phase 15: reinitialise AiController from new config
    // For now: store config and notify
    self.config = config;
    cx.notify();
}
```

---

## Step 7 — Title bar gear icon

In `Workspace::render` (app.rs), add a `⚙` button to the right side of the
title bar, between the namespace selector and the right edge:

```rust
TitleBar::new()
    .child(/* ... existing left side: logo + context/ns dropdowns ... */)
    .child(
        div()
            .flex()
            .items_center()
            .px_2()
            .child(
                div()
                    .cursor_pointer()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .hover(|s| s.bg(HOVER_BG))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|_, _, window, cx| {
                        cx.dispatch_action(Box::new(OpenSettings));
                    }))
                    .child(Label::new("⚙").text_sm().text_color(TEXT_MUTED))
            )
    )
```

---

## New dependency summary

| Crate | Version | Where | Reason |
|-------|---------|-------|--------|
| `toml` | `0.8` | `kairo-config` | TOML serialisation |
| `dirs` | `5` | `kairo-config` | Cross-platform home dir lookup |
| `kairo-config` | path | `kairo-ui` | Read/write settings |

`serde` and `thiserror` are already approved in `kairo-core` — same versions.

---

## What this phase does NOT include

- No test-connection button (added in Phase 15 when AI backends exist)
- No `[general]` section UI (reserved — schema stub is present in config)
- No migration logic (TOML `#[serde(default)]` handles missing fields gracefully)
- No macOS Keychain integration for API keys (plaintext `0600` file for now)

---

## Verification

```bash
cargo check                         # all crates compile
cargo clippy -- -D warnings         # clean
cargo test -p kairo-config          # load/save round-trip tests

# Manual:
cargo run -p kairo-ui
# 1. Click ⚙ in title bar (or Cmd+,)
# 2. Enter an API key, change a model name
# 3. Click Save
# 4. Verify ~/.kairo/config.toml was created with correct content
# 5. Restart app — settings persist
```

### Unit test sketch (`kairo-config/tests/round_trip.rs`)

```rust
#[test]
fn round_trip() {
    let cfg = KairoConfig {
        ai: AiConfig {
            active_provider: ActiveProvider::Anthropic,
            anthropic: AnthropicConfig {
                api_key: "sk-test".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    };
    let toml_str = toml::to_string_pretty(&cfg).unwrap();
    let parsed: KairoConfig = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.ai.anthropic.api_key, "sk-test");
    assert_eq!(parsed.ai.active_provider, ActiveProvider::Anthropic);
}
```

---

## Summary table

| Step | File | Change |
|------|------|--------|
| 1 | `Cargo.toml` | Add `kairo-config` workspace member |
| 2 | `crates/kairo-config/Cargo.toml` | New crate (`toml`, `dirs`, `serde`, `thiserror`) |
| 3 | `kairo-config/src/ai.rs` | `AiConfig` + 3 provider structs + defaults |
| 4 | `kairo-config/src/lib.rs` | `KairoConfig` + `load()` + `save()` + 0600 perms |
| 5 | `kairo-ui/src/actions.rs` | `OpenSettings` action |
| 6 | `kairo-ui/src/components/settings_panel.rs` | `SettingsPanel` modal + `SettingsSaved` event |
| 7 | `kairo-ui/src/components/mod.rs` | Register `settings_panel` |
| 8 | `kairo-ui/src/app.rs` | Gear icon + `OpenSettings` handler + `on_settings_saved` |
| 9 | `kairo-config/tests/round_trip.rs` | Load/save round-trip tests |
