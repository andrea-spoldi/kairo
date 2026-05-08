#!/usr/bin/env bash
# bump-version.sh - Update the workspace version in Cargo.toml and Cargo.lock.
# Called by semantic-release via @semantic-release/exec.
#
# Usage:
#   sh scripts/bump-version.sh <version>

set -eu

VERSION="${1:?Usage: bump-version.sh <version>}"

echo "Bumping version to ${VERSION}..."

# Update version = "..." inside [workspace.package] in Cargo.toml.
# Uses awk for macOS compatibility (BSD sed lacks multiline support).
awk -v ver="$VERSION" '
    /^\[workspace\.package\]/ { in_section=1 }
    /^\[/ && !/^\[workspace\.package\]/ { in_section=0 }
    in_section && /^version = / { $0 = "version = \"" ver "\"" }
    { print }
' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

# Update version = "..." for each workspace crate in Cargo.lock.
# Each crate has a [[package]] block; we match by name then patch the
# next version line within that block.
for pkg in kairo-config kairo-core kairo-ui; do
    awk -v pkg="$pkg" -v ver="$VERSION" '
        /^\[\[package\]\]/ { in_pkg=0 }
        /^name = /         { in_pkg = ($0 == "name = \"" pkg "\"") }
        in_pkg && /^version = / { $0 = "version = \"" ver "\"" }
        { print }
    ' Cargo.lock > Cargo.lock.tmp && mv Cargo.lock.tmp Cargo.lock
done

echo "Updated Cargo.toml and Cargo.lock to ${VERSION}."
