#!/usr/bin/env bash
# release.sh — Bump version, commit, tag, and push to trigger the release pipeline.
#
# Usage:
#   bash scripts/release.sh 0.4.0

set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: bash scripts/release.sh <version>"
  echo "Example: bash scripts/release.sh 0.4.0"
  exit 1
fi

TAG="v${VERSION}"
CARGO_TOML="Cargo.toml"

# ── Preflight ──────────────────────────────────────────────────────────────────
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "ERROR: Working tree has uncommitted changes. Commit or stash first."
  exit 1
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "ERROR: Tag $TAG already exists locally."
  exit 1
fi

# ── Bump version in Cargo.toml ────────────────────────────────────────────────
CURRENT=$(grep '^version = ' "$CARGO_TOML" | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "→  Bumping $CURRENT → $VERSION in $CARGO_TOML"
sed -i.bak "s/^version = \"${CURRENT}\"/version = \"${VERSION}\"/" "$CARGO_TOML"
rm -f "${CARGO_TOML}.bak"

# ── Commit + tag + push ───────────────────────────────────────────────────────
git add "$CARGO_TOML"
git commit -m "chore: bump version to ${VERSION}"
git tag "$TAG"

echo "→  Pushing branch and tag ${TAG}…"
git push origin HEAD
git push origin "$TAG"

echo "✓  Released ${TAG} — cargo-dist pipeline should now start."
