# Bug Log

Chronological log of bugs encountered and resolved. Keep entries brief.

## Format

Each entry: date, issue, root cause, solution, and optional prevention note.

---

### 2026-03-27 - GPUI git dep pulls entire Zed monorepo
- **Issue**: First `cargo build` takes very long or times out
- **Root Cause**: The `gpui` git dependency fetches the full Zed monorepo (~1GB+)
- **Solution**: Wait for the initial fetch to complete; subsequent builds use the cargo cache
- **Prevention**: Avoid `cargo clean` unnecessarily — it forces a full re-fetch

### 2026-03-27 - gpui + gpui-component version mismatch
- **Issue**: Compile errors when GPUI API changes after a git dep update
- **Root Cause**: `gpui-component` pins a specific GPUI revision; pulling HEAD of both causes drift
- **Solution**: Check `gpui-component`'s `Cargo.toml` for the `gpui` rev it expects, then pin `kairo-ui/Cargo.toml` to match
- **Prevention**: Always align revisions; never update one git dep without checking the other
