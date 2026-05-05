#!/usr/bin/env bash
# setup-vendor.sh — Recreate the vendored gpui-component directory.
#
# This MUST be run once after a fresh clone, before `cargo build`.
# The vendor/ directory is gitignored because it contains a source-level
# patch that cannot be upstreamed; this script reproduces it deterministically.
#
# What it does:
#   1. Clones gpui-component at the pinned rev used by Cargo.lock
#   2. Rewrites workspace-inherited Cargo.toml fields with explicit values
#      (gpui-component has its own workspace; placing it under kairo's workspace
#       would cause "value from workspace member manifests" errors otherwise)
#   3. Patches tab_panel.rs to hide the "Zoom In" context-menu entry when a
#      panel returns zoomable() = None (upstream always renders it disabled,
#      which clutters every panel menu with a greyed-out item)
#
# Usage:
#   bash scripts/setup-vendor.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$REPO_ROOT/vendor/gpui-component"
GPUI_COMPONENT_REV="f19f896fa193c57768eaf5403e4659737e3ba5ba"
GPUI_COMPONENT_VERSION="0.5.1"

# ── Guard ──────────────────────────────────────────────────────────────────────
if [[ -d "$VENDOR_DIR/.git" ]]; then
  CURRENT_REV="$(git -C "$VENDOR_DIR" rev-parse HEAD)"
  if [[ "$CURRENT_REV" == "$GPUI_COMPONENT_REV" ]]; then
    echo "✓  vendor/gpui-component already at correct rev. Nothing to do."
    exit 0
  fi
  echo "⚠  vendor/gpui-component exists but is at wrong rev ($CURRENT_REV)."
  echo "   Delete $VENDOR_DIR and re-run to recreate it."
  exit 1
fi

# ── Clone ──────────────────────────────────────────────────────────────────────
echo "→  Cloning gpui-component…"
git clone \
  --quiet \
  --no-checkout \
  --filter=blob:none \
  https://github.com/longbridge/gpui-component \
  "$VENDOR_DIR"

echo "→  Checking out rev $GPUI_COMPONENT_REV…"
git -C "$VENDOR_DIR" checkout --quiet "$GPUI_COMPONENT_REV"

# ── Fix workspace-inherited Cargo.toml fields ──────────────────────────────────
# gpui-component uses `version.workspace = true` etc. in its crate Cargo.tomls.
# Under kairo's workspace those keys resolve from kairo's [workspace.package],
# producing wrong values. Replace them with the explicit gpui-component values.
echo "→  Patching vendor Cargo.toml files (workspace → explicit)…"

for TOML in \
  "$VENDOR_DIR/crates/ui/Cargo.toml" \
  "$VENDOR_DIR/crates/assets/Cargo.toml"
do
  if [[ ! -f "$TOML" ]]; then
    echo "ERROR: expected file not found: $TOML" >&2
    exit 1
  fi
  sed -i.bak \
    -e "s|^version\.workspace\s*=\s*true|version = \"$GPUI_COMPONENT_VERSION\"|" \
    -e "s|^edition\.workspace\s*=\s*true|edition = \"2021\"|" \
    -e "s|^authors\.workspace\s*=\s*true|# authors removed|" \
    -e "s|^description\.workspace\s*=\s*true|# description removed|" \
    -e "s|^homepage\.workspace\s*=\s*true|# homepage removed|" \
    -e "s|^repository\.workspace\s*=\s*true|# repository removed|" \
    -e "s|^license\.workspace\s*=\s*true|license = \"MIT\"|" \
    "$TOML"
  rm -f "${TOML}.bak"
done

# ── Patch tab_panel.rs ─────────────────────────────────────────────────────────
# Upstream tab_panel.rs adds a "Zoom In" context-menu item unconditionally,
# then disables it when the panel is not zoomable. Kairo panels return None
# for zoomable(), so every right-click shows a useless greyed "Zoom In".
#
# The patch wraps the zoom entry in a `.when_some(zoomable, ...)` guard so it
# only appears when the panel actually supports zooming.
echo "→  Patching tab_panel.rs (conditional zoom menu item)…"

TAB_PANEL="$VENDOR_DIR/crates/ui/src/dock/tab_panel.rs"
if [[ ! -f "$TAB_PANEL" ]]; then
  echo "ERROR: tab_panel.rs not found at expected path: $TAB_PANEL" >&2
  exit 1
fi

# Check whether the patch is already applied (idempotent re-runs).
if grep -q 'when_some.*zoomable.*Zoom In\|Zoom In.*when_some.*zoomable' "$TAB_PANEL" 2>/dev/null; then
  echo "   tab_panel.rs already patched."
else
  python3 - "$TAB_PANEL" <<'PYEOF'
import sys, re

path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Pattern: a ContextMenuItem entry for "Zoom In" that is either:
#   a) always added (enabled/disabled based on zoomable)
#   b) already guarded but wrongly structured
#
# We look for a block like:
#   menu.entry("Zoom In", ...)
# or
#   .separator()
#   .entry("Zoom In", ...)
# and wrap the entry line(s) in .when_some(zoomable, |menu, _| menu<entry>)
#
# The exact upstream form at rev f19f896:
#
#   .separator()
#   .menu_item(
#       MenuItem::new("Zoom In")
#           .action(cx.handler_for::<ZoomIn>(...))
#           .disabled(zoomable.is_none()),
#   )
#
# We replace the `.disabled(...)` guard with a `.when_some` wrapping the
# entire menu_item call so the entry is absent rather than greyed out.

# Strategy: find the Zoom In menu_item block and prepend a when_some guard.
ZOOM_PATTERN = re.compile(
    r'(\s*)\.menu_item\(\s*\n'        # indent + .menu_item(
    r'\s*MenuItem::new\("Zoom In"\)'  # the Zoom In item
    r'.*?'                            # any options (non-greedy)
    r'\.disabled\(zoomable\.is_none\(\)\),?\s*\n'  # .disabled(zoomable.is_none())
    r'\s*\)',                         # closing )
    re.DOTALL
)

def replacer(m):
    indent = m.group(1)
    original = m.group(0)
    # Strip the .disabled(...) line from the inner call
    inner = re.sub(r'\s*\.disabled\(zoomable\.is_none\(\)\)', '', original)
    return (
        f"{indent}.when_some(zoomable, |menu, _| {{\n"
        f"{indent}    menu{inner.lstrip()}\n"
        f"{indent}}})"
    )

patched, count = ZOOM_PATTERN.subn(replacer, src)

if count == 0:
    # Fallback: simpler pattern — just a .disabled() on the Zoom In entry
    FALLBACK = re.compile(
        r'([ \t]*)(\.\s*entry\s*\(\s*"Zoom In"[^)]*\))\s*\n'
        r'([ \t]*\.disabled\(zoomable\.is_none\(\)\))',
        re.DOTALL
    )
    def fallback_replacer(m):
        indent = m.group(1)
        entry = m.group(2)
        return f"{indent}.when_some(zoomable, |menu, _| menu{entry})"
    patched, count = FALLBACK.subn(fallback_replacer, src)

if count == 0:
    print("WARNING: could not locate Zoom In pattern in tab_panel.rs.", file=sys.stderr)
    print("         The panel context menus will still show a greyed 'Zoom In'.", file=sys.stderr)
    print("         Inspect the file and apply the patch manually:", file=sys.stderr)
    print(f"         {path}", file=sys.stderr)
    sys.exit(0)  # non-fatal: build still works, just cosmetic

with open(path, 'w') as f:
    f.write(patched)

print(f"   Applied {count} substitution(s).")
PYEOF
fi

echo ""
echo "✓  vendor/gpui-component is ready."
echo "   You can now run: cargo build --release -p kairo-ui"
