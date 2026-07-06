#!/usr/bin/env bash
#
# sync-version.sh - surgically update the version marker in the config files
# that track the release version:
#
#   - package.json                  →   "version": "X.Y.Z"
#   - src-tauri/tauri.conf.json     →   "version": "X.Y.Z"
#   - src-tauri/Cargo.toml          →   version = "X.Y.Z"
#   - README.md                     →   download URLs + visible filenames
#
# Why surgical sed (and NOT JSON.parse / jq / a full rewrite)?
#   package.json and tauri.conf.json both contain intentionally-compact arrays
#   (e.g. "targets": ["nsis", "msi", "deb", "appimage", "dmg"]). Re-parsing and
#   re-serializing with `JSON.stringify(..., null, 2)` or `jq` expands those
#   arrays onto multiple lines, producing a noisy diff unrelated to the version.
#   This script replaces ONLY the single version line in each file and leaves
#   every other byte untouched.
#
# README.md handling:
#   Two sed passes keep the GitHub Releases download URLs and the visible
#   filenames in lockstep with the version, so the README never points at a
#   stale release:
#     1. /releases/download/vX.Y.Z/   →   /releases/download/vNEW/
#     2. Bango_X.Y.Z_  (underscore)   →   Bango_NEW_   (AppImage/deb/dmg/exe/msi)
#   The MSIX bundle (Bango_X.Y.Z.0.msixbundle) is intentionally not linked
#   from the README, so no fourth pass is needed.
#
# Portability:
#   `sed -i` is incompatible across GNU sed (Linux/Git-Bash) and BSD sed
#   (macOS). We instead write to a temp file and atomically `mv` it back,
#   which behaves identically everywhere.
#
# Usage:
#   bash scripts/sync-version.sh "<version>"     e.g.  bash scripts/sync-version.sh "2.1.0"
#
# Exit codes:
#   0  all files updated (or already at the target version)
#   1  missing / invalid version argument
#   2  a version line could not be found in one of the files

set -euo pipefail

if [ "$#" -ne 1 ] || [ -z "${1:-}" ]; then
  echo "Usage: $0 <version>   (e.g. 1.2.3)" >&2
  exit 1
fi

VERSION="$1"

# Resolve the repository root from this script's location so the script can be
# invoked from any working directory (CI runs it from the checkout root, but
# local use should work too).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# update_json <file>
# Replaces the first top-level  "version": "..."  line (2-space indent, as
# enforced by Prettier on both JSON files).
update_json() {
  local file="$1"
  local tmp
  tmp="$(mktemp)"
  # Anchor on `^  "version":` so we never match a nested `version` key (e.g.
  # the dependency entries in package.json).
  sed 's|^  "version": ".*"|  "version": "'"$VERSION"'"|' "$file" > "$tmp"
  if ! grep -q "^  \"version\": \"$VERSION\"" "$tmp"; then
    echo "Error: version line not updated in $file" >&2
    rm -f "$tmp"
    exit 2
  fi
  mv "$tmp" "$file"
}

# update_cargo <file>
# Replaces the first  version = "..."  line in [package].
update_cargo() {
  local file="$1"
  local tmp
  tmp="$(mktemp)"
  sed 's|^version = ".*"|version = "'"$VERSION"'"|' "$file" > "$tmp"
  if ! grep -q "^version = \"$VERSION\"" "$tmp"; then
    echo "Error: version line not updated in $file" >&2
    rm -f "$tmp"
    exit 2
  fi
  mv "$tmp" "$file"
}

# update_readme <file>
# Bumps every GitHub Releases download URL and visible filename version in
# README.md. Two sed passes:
#   1. `releases/download/vOLD/` → `releases/download/vNEW/`
#   2. `Bango_OLD_`              → `Bango_NEW_`   (visible filename text)
# Pass 1 covers the URL; pass 2 covers the link text inside the backticks.
# The underscore anchor means non-versioning underscores (e.g. in prose) are
# untouched: the pattern only matches `Bango_<digits-and-dots>_`.
update_readme() {
  local file="$1"
  local tmp
  tmp="$(mktemp)"
  # Pass 1: release URL tag segment (vX.Y.Z → vNEW).
  sed 's|/releases/download/v[0-9][0-9.]*[0-9]/|/releases/download/v'"$VERSION"'/|g' "$file" > "$tmp"
  # Pass 2: visible filename prefix Bango_X.Y.Z_ → Bango_NEW_ (in-place on tmp).
  sed 's|Bango_[0-9][0-9.]*[0-9]_|Bango_'"$VERSION"'_|g' "$tmp" > "$file"
  rm -f "$tmp"
  # Informational only - do not fail the build if README has no version URLs
  # yet (e.g. a fresh fork that deleted the download table).
  if ! grep -q "releases/download/v$VERSION/" "$file"; then
    echo "Note: no 'releases/download/v$VERSION/' URLs found in $file (skipped)" >&2
  fi
}

cd "$ROOT_DIR"

update_json "package.json"
update_json "src-tauri/tauri.conf.json"
update_cargo "src-tauri/Cargo.toml"
update_readme "README.md"

echo "Synced version to $VERSION across package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml, README.md"