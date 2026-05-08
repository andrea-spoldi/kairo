#!/usr/bin/env bash
# setup-vendor.sh - Recreate the vendored gpui-component directory.
#
# Run once after a fresh clone, before `cargo build`.
# vendor/ is gitignored because it contains a source-level patch that
# cannot be upstreamed; this script reproduces it deterministically.
#
# What it does:
#   1. Clones gpui-component at the pinned rev used by Cargo.lock
#   2. Rewrites workspace-inherited fields in the crate Cargo.tomls with
#      explicit values so they build under kairo's workspace
#   3. Patches tab_panel.rs to hide the "Zoom In" context-menu entry when a
#      panel returns zoomable() = None
#
# Usage:
#   bash scripts/setup-vendor.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="${REPO_ROOT}/vendor/gpui-component"
GPUI_COMPONENT_REV="f19f896fa193c57768eaf5403e4659737e3ba5ba"

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

# -- Patch Cargo.toml files ----------------------------------------------------
# gpui-component crates use `dep.workspace = true` / `dep = { workspace = true }`
# throughout their Cargo.tomls. Under kairo's workspace those references fail
# because kairo's Cargo.toml doesn't define [workspace.dependencies].
#
# This Python script replaces every workspace dep reference with the explicit
# version from gpui-component's own [workspace.dependencies] (Cargo.toml root).
echo "Patching vendor Cargo.toml files (workspace -> explicit)..."

python3 - \
  "${VENDOR_DIR}/crates/ui/Cargo.toml" \
  "${VENDOR_DIR}/crates/assets/Cargo.toml" \
  "${VENDOR_DIR}/crates/macros/Cargo.toml" \
<<'PYEOF'
import re, sys

# Explicit specs for every dep gpui-component crates inherit via workspace.
# Sourced from gpui-component's Cargo.toml [workspace.dependencies] at rev f19f896.
WORKSPACE_DEPS = {
    'anyhow':                 '"1"',
    'log':                    '"0.4"',
    'lsp-types':              '{ version = "0.97.0", features = ["proposed"] }',
    'notify':                 '"7.0.0"',
    'ropey':                  '{ version = "=2.0.0-beta.1", features = ["metric_lines_lf", "metric_utf16"] }',
    'rust-i18n':              '"3"',
    'schemars':               '"1"',
    'serde':                  '{ version = "1.0.219", features = ["derive"] }',
    'serde_json':             '"1"',
    'serde_repr':             '"0.1"',
    'smallvec':               '"1"',
    'smol':                   '"2"',
    'sum-tree':               '{ version = "0.2.0", package = "zed-sum-tree" }',
    'tracing':                '"0.1.41"',
    'wasm-bindgen':           '"0.2.113"',
    'gpui':                   '{ git = "https://github.com/zed-industries/zed" }',
    'gpui_macros':            '{ git = "https://github.com/zed-industries/zed" }',
    'gpui-component-macros':  '{ path = "../macros", version = "0.5.1" }',
    # wasm-only; not compiled for macOS/Linux but must be syntactically valid
    'reqwest': (
        '{ git = "https://github.com/zed-industries/reqwest.git"'
        ', rev = "c15662463bda39148ba154100dd44d3fba5873a4"'
        ', default-features = false'
        ', features = ["charset","http2","macos-system-configuration","multipart"'
        ',"rustls-tls-native-roots","socks","stream"]'
        ', package = "zed-reqwest", version = "0.12.15-zed" }'
    ),
}

def patch(path):
    with open(path) as f:
        src = f.read()

    # 1. Dotted package field: `edition.workspace = true`
    def replace_pkg_field(m):
        key = m.group(1)
        defaults = {'edition': '"2024"', 'license': '"Apache-2.0"'}
        return f'{key} = {defaults.get(key, "# removed")}' if key not in WORKSPACE_DEPS else m.group(0)
    src = re.sub(r'^([\w-]+)\.workspace\s*=\s*true\s*$', replace_pkg_field, src, flags=re.MULTILINE)

    # 2. Dotted dep: `anyhow.workspace = true`
    def replace_dotted_dep(m):
        name = m.group(1)
        return f'{name} = {WORKSPACE_DEPS[name]}' if name in WORKSPACE_DEPS else m.group(0)
    src = re.sub(r'^([\w_-]+)\.workspace\s*=\s*true\s*$', replace_dotted_dep, src, flags=re.MULTILINE)

    # 3. Inline table, workspace only: `name = { workspace = true }`
    def replace_inline_simple(m):
        name = m.group(1)
        return f'{name} = {WORKSPACE_DEPS[name]}' if name in WORKSPACE_DEPS else m.group(0)
    src = re.sub(r'^([\w_-]+)\s*=\s*\{\s*workspace\s*=\s*true\s*\}\s*$',
                 replace_inline_simple, src, flags=re.MULTILINE)

    # 4. Inline table with extra fields: `name = { workspace = true, features = [...] }`
    def replace_inline_extra(m):
        name, rest = m.group(1), m.group(2)
        if name not in WORKSPACE_DEPS:
            return m.group(0)
        base = WORKSPACE_DEPS[name]
        extra = re.sub(r'\bworkspace\s*=\s*true,?\s*', '', rest).strip().strip(',').strip()
        if not extra:
            return f'{name} = {base}'
        if base.startswith('"'):
            return f'{name} = {{ version = {base}, {extra} }}'
        # base is an inline table: insert extra before closing brace
        return f'{name} = {{ {base[1:-1].rstrip().rstrip(",")}, {extra} }}'
    src = re.sub(r'^([\w_-]+)\s*=\s*\{([^}]*workspace\s*=\s*true[^}]*)\}\s*$',
                 replace_inline_extra, src, flags=re.MULTILINE)

    # 5. Remove [lints] workspace = true section (not valid under kairo workspace)
    src = re.sub(r'\[lints\]\s*\nworkspace\s*=\s*true\n?', '', src)

    with open(path, 'w') as f:
        f.write(src)
    print(f"  patched {path}")

for p in sys.argv[1:]:
    patch(p)
PYEOF

# -- Patch tab_panel.rs --------------------------------------------------------
# The dropdown menu in tab_panel always renders a Zoom In/Out item, disabling
# it via menu_with_disabled() when the panel is not zoomable. Kairo panels
# return None for zoomable(), so every panel menu shows a greyed Zoom item.
#
# Fix: wrap the separator + zoom entry in .when(zoomable, ...) so the item is
# absent entirely instead of being present but disabled.
echo "Patching tab_panel.rs (conditional zoom menu item)..."

TAB_PANEL="${VENDOR_DIR}/crates/ui/src/dock/tab_panel.rs"
if [[ ! -f "${TAB_PANEL}" ]]; then
  echo "ERROR: tab_panel.rs not found: ${TAB_PANEL}" >&2
  exit 1
fi

python3 - "${TAB_PANEL}" <<'PYEOF'
import sys

path = sys.argv[1]
with open(path) as f:
    src = f.read()

# Already patched (idempotent re-runs)
if '.when(zoomable' in src:
    print("  tab_panel.rs already patched.")
    sys.exit(0)

OLD = (
    '                                    .separator()\n'
    '                                    .menu_with_disabled(\n'
    '                                        if zoomed {\n'
    '                                            t!("Dock.Zoom Out")\n'
    '                                        } else {\n'
    '                                            t!("Dock.Zoom In")\n'
    '                                        },\n'
    '                                        Box::new(ToggleZoom),\n'
    '                                        !zoomable,\n'
    '                                    )'
)

NEW = (
    '                                    .when(zoomable, |this| {\n'
    '                                        this.separator()\n'
    '                                            .menu(\n'
    '                                                if zoomed {\n'
    '                                                    t!("Dock.Zoom Out")\n'
    '                                                } else {\n'
    '                                                    t!("Dock.Zoom In")\n'
    '                                                },\n'
    '                                                Box::new(ToggleZoom),\n'
    '                                            )\n'
    '                                    })'
)

if OLD not in src:
    print("WARNING: expected Zoom In pattern not found in tab_panel.rs.", file=sys.stderr)
    print("         Panel menus may show a greyed 'Zoom In' item.", file=sys.stderr)
    print("         Inspect and patch manually: " + path, file=sys.stderr)
    sys.exit(0)  # non-fatal: build still works

patched = src.replace(OLD, NEW, 1)
with open(path, 'w') as f:
    f.write(patched)
print("  tab_panel.rs patched.")
PYEOF

echo ""
echo "Done. vendor/gpui-component is ready."
echo "Run: cargo build --release -p kairo-ui"
