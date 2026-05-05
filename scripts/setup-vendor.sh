#!/usr/bin/env bash
# setup-vendor.sh - Recreate the vendored gpui-component directory.
#
# Run once after a fresh clone, before `cargo build`.
# vendor/ is gitignored because it contains a source-level patch that
# cannot be upstreamed; this script reproduces it deterministically.
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
VENDOR_DIR="${REPO_ROOT}/vendor/gpui-component"
GPUI_COMPONENT_REV="f19f896fa193c57768eaf5403e4659737e3ba5ba"
GPUI_COMPONENT_VERSION="0.5.1"

# -- Guard ---------------------------------------------------------------------
if [[ -d "${VENDOR_DIR}/.git" ]]; then
  CURRENT_REV="$(git -C "${VENDOR_DIR}" rev-parse HEAD)"
  if [[ "${CURRENT_REV}" == "${GPUI_COMPONENT_REV}" ]]; then
    echo "vendor/gpui-component already at correct rev. Nothing to do."
    exit 0
  fi
  echo "ERROR: vendor/gpui-component exists but is at wrong rev (${CURRENT_REV})."
  echo "       Delete ${VENDOR_DIR} and re-run to recreate it."
  exit 1
fi

# -- Clone ---------------------------------------------------------------------
echo "Cloning gpui-component..."
git clone \
  --quiet \
  --no-checkout \
  --filter=blob:none \
  https://github.com/longbridge/gpui-component \
  "${VENDOR_DIR}"

echo "Checking out rev ${GPUI_COMPONENT_REV}..."
git -C "${VENDOR_DIR}" checkout --quiet "${GPUI_COMPONENT_REV}"

# -- Fix workspace-inherited Cargo.toml fields ---------------------------------
# gpui-component uses `version.workspace = true` etc. in its crate Cargo.tomls.
# Under kairo's workspace those keys resolve from kairo's [workspace.package],
# producing wrong values. Replace them with the explicit gpui-component values.
echo "Patching vendor Cargo.toml files (workspace -> explicit)..."

for TOML in \
  "${VENDOR_DIR}/crates/ui/Cargo.toml" \
  "${VENDOR_DIR}/crates/assets/Cargo.toml"
do
  if [[ ! -f "${TOML}" ]]; then
    echo "ERROR: expected file not found: ${TOML}" >&2
    exit 1
  fi
  sed -i.bak \
    -e "s|^version\\.workspace\\s*=\\s*true|version = \"${GPUI_COMPONENT_VERSION}\"|" \
    -e "s|^edition\\.workspace\\s*=\\s*true|edition = \"2021\"|" \
    -e "s|^authors\\.workspace\\s*=\\s*true|# authors removed|" \
    -e "s|^description\\.workspace\\s*=\\s*true|# description removed|" \
    -e "s|^homepage\\.workspace\\s*=\\s*true|# homepage removed|" \
    -e "s|^repository\\.workspace\\s*=\\s*true|# repository removed|" \
    -e "s|^license\\.workspace\\s*=\\s*true|license = \"MIT\"|" \
    "${TOML}"
  rm -f "${TOML}.bak"
done

# -- Patch tab_panel.rs --------------------------------------------------------
# Upstream tab_panel.rs adds a "Zoom In" context-menu item unconditionally,
# then disables it when the panel is not zoomable. Kairo panels return None
# for zoomable(), so every right-click shows a useless greyed "Zoom In".
#
# The patch wraps the zoom entry in a .when_some(zoomable, ...) guard so it
# only appears when the panel actually supports zooming.
echo "Patching tab_panel.rs (conditional zoom menu item)..."

TAB_PANEL="${VENDOR_DIR}/crates/ui/src/dock/tab_panel.rs"
if [[ ! -f "${TAB_PANEL}" ]]; then
  echo "ERROR: tab_panel.rs not found at expected path: ${TAB_PANEL}" >&2
  exit 1
fi

# Check whether the patch is already applied (idempotent re-runs).
if grep -q 'when_some.*Zoom In\|Zoom In.*when_some' "${TAB_PANEL}" 2>/dev/null; then
  echo "tab_panel.rs already patched."
else
  python3 - "${TAB_PANEL}" <<'PYEOF'
import sys, re

path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Upstream form at rev f19f896:
#
#   .menu_item(
#       MenuItem::new("Zoom In")
#           .action(cx.handler_for::<ZoomIn>(...))
#           .disabled(zoomable.is_none()),
#   )
#
# We remove the .disabled(...) guard and wrap the whole block in
# .when_some(zoomable, ...) so the entry is absent rather than greyed out.

ZOOM_PATTERN = re.compile(
    r'(\s*)\.menu_item\(\s*\n'
    r'\s*MenuItem::new\("Zoom In"\)'
    r'.*?'
    r'\.disabled\(zoomable\.is_none\(\)\),?\s*\n'
    r'\s*\)',
    re.DOTALL
)

def replacer(m):
    indent = m.group(1)
    original = m.group(0)
    inner = re.sub(r'\s*\.disabled\(zoomable\.is_none\(\)\)', '', original)
    return (
        indent + '.when_some(zoomable, |menu, _| {\n' +
        indent + '    menu' + inner.lstrip() + '\n' +
        indent + '})'
    )

patched, count = ZOOM_PATTERN.subn(replacer, src)

if count == 0:
    # Fallback: simpler .entry() form
    FALLBACK = re.compile(
        r'([ \t]*)(\.entry\s*\(\s*"Zoom In"[^)]*\))\s*\n'
        r'([ \t]*\.disabled\(zoomable\.is_none\(\)\))',
        re.DOTALL
    )
    def fallback_replacer(m):
        indent = m.group(1)
        entry = m.group(2)
        return indent + '.when_some(zoomable, |menu, _| menu' + entry + ')'
    patched, count = FALLBACK.subn(fallback_replacer, src)

if count == 0:
    print("WARNING: could not locate Zoom In pattern in tab_panel.rs.", file=sys.stderr)
    print("         Panel menus will still show a greyed 'Zoom In' item.", file=sys.stderr)
    print("         Apply the patch manually: " + path, file=sys.stderr)
    sys.exit(0)  # non-fatal: build still works, cosmetic only

with open(path, 'w') as f:
    f.write(patched)

print("Applied " + str(count) + " substitution(s).")
PYEOF
fi

echo ""
echo "Done. vendor/gpui-component is ready."
echo "Run: cargo build --release -p kairo-ui"
