#!/usr/bin/env bash
# make-icons.sh — Convert assets/icons/kairo.png to an ICNS file
#                 ready for the macOS .app bundle.
#
# Prerequisites: macOS only (uses sips + iconutil, both built-in)
#
# Usage:
#   bash scripts/make-icons.sh
#
# Output:
#   crates/kairo-ui/assets/icons/kairo.icns

set -euo pipefail

PNG="assets/icons/kairo.png"
ICONSET="build/Kairo.iconset"
OUT="crates/kairo-ui/assets/icons/kairo.icns"

# ── Preflight ──────────────────────────────────────────────────────────────────
if [[ ! -f "$PNG" ]]; then
  echo "ERROR: $PNG not found. Place your PNG icon there and re-run."
  exit 1
fi

# ── Generate PNG slices via sips (built-in macOS tool) ────────────────────────
mkdir -p "$ICONSET"

declare -a SIZES=(16 32 128 256 512)
for size in "${SIZES[@]}"; do
  sips -z "$size" "$size" "$PNG" --out "${ICONSET}/icon_${size}x${size}.png"    >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$PNG" --out "${ICONSET}/icon_${size}x${size}@2x.png" >/dev/null
  echo "  ${size}x${size} + @2x"
done

# ── Bundle into .icns ──────────────────────────────────────────────────────────
mkdir -p "$(dirname "$OUT")"
iconutil -c icns "$ICONSET" -o "$OUT"
rm -rf "$ICONSET"

echo "✓  Generated $OUT"
