# CLAUDE.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.

# Kairo

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
from raw GPUI primitives. The `kairo-core` crate is unaffected by this
choice since it has zero UI dependencies.

## Architecture

Two-crate workspace. The K8s layer has **zero** UI dependencies.

```
kairo/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── kairo-core/         # K8s client, watchers, models (NO gpui imports)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs       # kubeconfig loading, context switching
│   │       ├── watchers.rs     # kube::runtime::watcher-based streams
│   │       ├── models.rs       # PodSummary, PodDetail, ContainerStatus, PodEvent
│   │       └── logs.rs         # log follow stream abstraction
│   └── kairo-ui/           # GPUI application
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

- `kairo-core` must NEVER import `gpui`. If you feel the urge, you’re putting UI logic in the wrong crate.
- All K8s API interaction goes through `kairo-core`. The UI crate calls core functions, never constructs `Api<T>` directly.
- Pod list updates use `kube::runtime::watcher` (event-driven). No polling loops.
- No `.unwrap()` in non-test code. Propagate errors with `?` or handle explicitly.

## Dependencies

Approved dependency list. Do NOT add others without asking.

**UI crate (`kairo-ui`):**

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

**Core crate (`kairo-core`):**

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
cargo test -p kairo-core          # unit tests
cargo test -p kairo-core --features integration  # needs a live cluster
cargo run -p kairo-ui             # launch the app
```

## Code Style

- Rust 2021 edition
- Doc comments (`///`) on all public types and functions in `kairo-core`
- `#[derive(Debug, Clone)]` on all data model structs
- Error types use `thiserror` with human-readable messages
- Prefer `impl Into<SharedString>` for GPUI string params
- snake_case functions, CamelCase types, SCREAMING_SNAKE_CASE constants
- Keep functions under 50 lines; extract if longer

## Git

- Init repo on first scaffold
- Conventional Commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`
- Commit after each completed phase, not after every file

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

## Project Memory System

Institutional knowledge lives in `docs/project_notes/` for consistency across sessions.

### Memory Files

- **bugs.md** — Bug log with dates, solutions, and prevention notes
- **decisions.md** — Architectural Decision Records (ADRs) with context and trade-offs
- **key_facts.md** — Project configuration, approved deps, important URLs, phase progress
- **issues.md** — Work log with brief descriptions of completed phases/tasks

### Memory-Aware Protocols

**Before proposing architectural changes:**
- Check `docs/project_notes/decisions.md` for existing decisions
- If the proposal conflicts, acknowledge the existing ADR and explain why revisiting it is warranted

**When encountering errors or bugs:**
- Search `docs/project_notes/bugs.md` for similar issues before diagnosing
- Apply known solutions if found; document new bugs and solutions when resolved

**When looking up project configuration or approved deps:**
- Check `docs/project_notes/key_facts.md` first

**When completing a phase or significant task:**
- Log it in `docs/project_notes/issues.md` with date and brief description
- Update the "Phase Progress" section in `key_facts.md`

**Style guidelines for memory files:**
- Bullet lists, not tables
- Always include dates (YYYY-MM-DD)
- Keep entries to 1–3 lines; link to code/commits for details
- Manual cleanup of old entries is expected (not automated)

<!-- code-review-graph MCP tools -->
## MCP Tools: code-review-graph

**IMPORTANT: This project has a knowledge graph. ALWAYS use the
code-review-graph MCP tools BEFORE using Grep/Glob/Read to explore
the codebase.** The graph is faster, cheaper (fewer tokens), and gives
you structural context (callers, dependents, test coverage) that file
scanning cannot.

### When to use graph tools FIRST

- **Exploring code**: `semantic_search_nodes` or `query_graph` instead of Grep
- **Understanding impact**: `get_impact_radius` instead of manually tracing imports
- **Code review**: `detect_changes` + `get_review_context` instead of reading entire files
- **Finding relationships**: `query_graph` with callers_of/callees_of/imports_of/tests_for
- **Architecture questions**: `get_architecture_overview` + `list_communities`

Fall back to Grep/Glob/Read **only** when the graph doesn't cover what you need.

### Key Tools

| Tool | Use when |
|------|----------|
| `detect_changes` | Reviewing code changes — gives risk-scored analysis |
| `get_review_context` | Need source snippets for review — token-efficient |
| `get_impact_radius` | Understanding blast radius of a change |
| `get_affected_flows` | Finding which execution paths are impacted |
| `query_graph` | Tracing callers, callees, imports, tests, dependencies |
| `semantic_search_nodes` | Finding functions/classes by name or keyword |
| `get_architecture_overview` | Understanding high-level codebase structure |
| `refactor_tool` | Planning renames, finding dead code |

### Workflow

1. The graph auto-updates on file changes (via hooks).
2. Use `detect_changes` for code review.
3. Use `get_affected_flows` to understand impact.
4. Use `query_graph` pattern="tests_for" to check coverage.
