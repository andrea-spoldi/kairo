#!/usr/bin/env bash
# make-dmg.sh — Build a release .app bundle and wrap it in a drag-to-install DMG.
#
# Prerequisites (macOS):
#   cargo install cargo-bundle    # https://github.com/burtonageo/cargo-bundle
#   brew install create-dmg       # https://github.com/create-dmg/create-dmg
#
# Usage:
#   bash scripts/make-dmg.sh              # uses version from Cargo.toml
#   bash scripts/make-dmg.sh 1.2.3        # override version label

set -euo pipefail

# ── Version ────────────────────────────────────────────────────────────────────
if [[ "${1:-}" != "" ]]; then
  VERSION="$1"
else
  VERSION=$(cargo metadata --no-deps --format-version 1 \
    | python3 -c "import json,sys; d=json.load(sys.stdin); \
      print(next(p['version'] for p in d['packages'] if p['name']=='kairo-ui'))")
fi

DMG_NAME="Kairo-${VERSION}-macos.dmg"
APP_PATH="target/release/bundle/osx/Kairo.app"
ICNS="crates/kairo-ui/assets/icons/kairo.icns"

# ── Preflight ──────────────────────────────────────────────────────────────────
if ! command -v cargo-bundle &>/dev/null; then
  echo "ERROR: cargo-bundle not found. Install with: cargo install cargo-bundle"
  exit 1
fi
if ! command -v create-dmg &>/dev/null; then
  echo "ERROR: create-dmg not found. Install with: brew install create-dmg"
  exit 1
fi
if [[ ! -f "$ICNS" ]]; then
  echo "WARN: $ICNS not found — run scripts/make-icons.sh first."
  echo "      Continuing without volume icon…"
  ICNS=""
fi

# ── Build .app ─────────────────────────────────────────────────────────────────
echo "→  Building Kairo.app (release)…"
cargo bundle --release -p kairo-ui

# ── Create DMG ────────────────────────────────────────────────────────────────
echo "→  Packaging ${DMG_NAME}…"
rm -f "${DMG_NAME}"

DMG_ARGS=(
  --volname "Kairo ${VERSION}"
  --window-pos 200 120
  --window-size 620 420
  --icon-size 100
  --icon "Kairo.app" 175 200
  --hide-extension "Kairo.app"
  --app-drop-link 440 200
  --no-internet-enable
)
[[ -n "$ICNS" ]] && DMG_ARGS+=(--volicon "$ICNS")

create-dmg "${DMG_ARGS[@]}" "${DMG_NAME}" "${APP_PATH}"

echo "✓  ${DMG_NAME}"
