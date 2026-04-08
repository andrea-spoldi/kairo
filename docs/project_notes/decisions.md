# Architectural Decisions

Lightweight ADR log. Full context lives here; implementation details live in the code.

---

### ADR-001: Two-crate workspace — kairo-core (no UI) + kairo-ui (2026-03-27)

**Context:**
- Want to unit-test K8s logic without a display server
- Want the option to add a CLI or alternative front-end later

**Decision:**
- Split into `kairo-core` (K8s client, watchers, models — zero GPUI imports) and `kairo-ui` (GPUI app)
- Hard rule: `kairo-core` must never import `gpui`

**Alternatives Considered:**
- Single crate → Rejected: would entangle K8s logic with UI, making tests harder
- Three crates (core / ui / app) → Rejected: unnecessary complexity at this stage

**Consequences:**
- ✅ Clean separation; core testable without display
- ✅ UI crate can be swapped or extended without touching K8s logic
- ❌ Slightly more boilerplate (workspace + two `Cargo.toml` files)

---

### ADR-002: Use git deps for GPUI (from Zed monorepo) + gpui-component (Longbridge) (2026-03-27)

**Context:**
- `gpui` on crates.io is a stale snapshot (0.2.2); `gpui-ce` is 381+ commits behind
- `gpui-component` gives 60+ ready-made components and recommends pointing at the Zed monorepo

**Decision:**
- Use `git` dependencies for both `gpui` (zed-industries/zed) and `gpui-component` (longbridge/gpui-component)
- Pin to compatible revisions when builds break

**Alternatives Considered:**
- `gpui-ce` from crates.io → Fallback only; too far behind, incompatible with gpui-component
- Raw GPUI primitives without gpui-component → More work; only if git dep situation becomes unmanageable

**Consequences:**
- ✅ Access to latest GPUI features and gpui-component library
- ❌ Large initial fetch (entire Zed monorepo)
- ❌ Must manually align revisions when either dep updates

---

### ADR-003: kube-rs with event-driven watchers (no polling) (2026-03-27)

**Context:**
- Want real-time pod list updates without spinning CPU on polling loops
- `kube::runtime::watcher` provides exactly this via the K8s watch API

**Decision:**
- All pod/namespace list updates use `kube::runtime::watcher` feeding a `tokio::sync::mpsc` channel
- No `sleep`-based polling loops anywhere in `kairo-core`

**Alternatives Considered:**
- Polling every N seconds → Rejected: wastes API calls, laggy, hard to cancel cleanly

**Consequences:**
- ✅ Low latency updates, CPU-efficient
- ✅ Cleaner cancellation via channel drop
- ❌ Slightly more complex setup than a simple list-and-refresh loop
