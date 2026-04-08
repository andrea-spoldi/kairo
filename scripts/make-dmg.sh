#!/usr/bin/env bash
# make-dmg.sh — Build a release .app bundle and wrap it in a drag-to-install DMG.
#
# Prerequisites (macOS):
#   brew install create-dmg
#   (no cargo-bundle needed — we build the .app manually for reliability)
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

APP="Kairo.app"
DMG_NAME="Kairo-${VERSION}-macos.dmg"
ICNS="crates/kairo-ui/assets/icons/kairo.icns"
BINARY="target/release/kairo"

# ── Preflight ──────────────────────────────────────────────────────────────────
if ! command -v create-dmg &>/dev/null; then
  echo "ERROR: create-dmg not found. Install with: brew install create-dmg"
  exit 1
fi
if [[ ! -f "$ICNS" ]]; then
  echo "ERROR: $ICNS not found — run scripts/make-icons.sh first."
  exit 1
fi

# ── Build binary ───────────────────────────────────────────────────────────────
echo "→  Building release binary…"
cargo build --release -p kairo-ui

# ── Assemble .app bundle manually ─────────────────────────────────────────────
# cargo-bundle is unreliable with icons; building the bundle ourselves
# gives us full control over Info.plist and Resources layout.
echo "→  Assembling ${APP}…"
rm -rf "${APP}"
mkdir -p "${APP}/Contents/MacOS"
mkdir -p "${APP}/Contents/Resources"

cp "${BINARY}" "${APP}/Contents/MacOS/kairo"
cp "${ICNS}"   "${APP}/Contents/Resources/kairo.icns"

cat > "${APP}/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>   <string>Kairo</string>
  <key>CFBundleExecutable</key>    <string>kairo</string>
  <key>CFBundleIconFile</key>      <string>kairo</string>
  <key>CFBundleIdentifier</key>    <string>dev.kairo.app</string>
  <key>CFBundleName</key>          <string>Kairo</string>
  <key>CFBundlePackageType</key>   <string>APPL</string>
  <key>CFBundleShortVersionString</key> <string>${VERSION}</string>
  <key>CFBundleVersion</key>       <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2026 Andrea Spoldi. All rights reserved.</string>
</dict>
</plist>
PLIST

# Touch .app so Finder picks up the new icon immediately.
touch "${APP}"

# ── Create DMG ────────────────────────────────────────────────────────────────
echo "→  Packaging ${DMG_NAME}…"
rm -f "${DMG_NAME}"

create-dmg \
  --volname "Kairo ${VERSION}" \
  --volicon "${ICNS}" \
  --window-pos 200 120 \
  --window-size 620 420 \
  --icon-size 100 \
  --icon "Kairo.app" 175 200 \
  --hide-extension "Kairo.app" \
  --app-drop-link 440 200 \
  --no-internet-enable \
  "${DMG_NAME}" \
  "${APP}"

# Clean up the staging .app (the DMG contains a copy).
rm -rf "${APP}"

echo "✓  ${DMG_NAME}"
