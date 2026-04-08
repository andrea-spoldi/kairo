#!/usr/bin/env bash
# make-icons.sh — Convert assets/icons/kairo.svg to an ICNS file
#                 ready for the macOS .app bundle.
#
# Prerequisites (macOS):
#   brew install librsvg          # provides rsvg-convert
#   iconutil is built into macOS
#
# Usage:
#   bash scripts/make-icons.sh
#
# Output:
#   crates/kairo-ui/assets/icons/kairo.icns

set -euo pipefail

SVG="assets/icons/kairo.svg"
ICONSET="build/Kairo.iconset"
OUT="crates/kairo-ui/assets/icons/kairo.icns"

# ── Preflight ──────────────────────────────────────────────────────────────────
if [[ ! -f "$SVG" ]]; then
  echo "ERROR: $SVG not found."
  echo "       Place your SVG icon at assets/icons/kairo.svg and re-run."
  exit 1
fi
if ! command -v rsvg-convert &>/dev/null; then
  echo "ERROR: rsvg-convert not found. Install with: brew install librsvg"
  exit 1
fi
if ! command -v iconutil &>/dev/null; then
  echo "ERROR: iconutil not found (macOS only)."
  exit 1
fi

# ── Generate PNG slices ────────────────────────────────────────────────────────
mkdir -p "$ICONSET"

declare -a SIZES=(16 32 128 256 512)
for size in "${SIZES[@]}"; do
  rsvg-convert -w "$size"        -h "$size"        "$SVG" -o "${ICONSET}/icon_${size}x${size}.png"
  rsvg-convert -w $((size * 2)) -h $((size * 2)) "$SVG" -o "${ICONSET}/icon_${size}x${size}@2x.png"
  echo "  ${size}x${size} + @2x"
done

# ── Bundle into .icns ──────────────────────────────────────────────────────────
mkdir -p "$(dirname "$OUT")"
iconutil -c icns "$ICONSET" -o "$OUT"
rm -rf "$ICONSET"

echo "✓  Generated $OUT"
