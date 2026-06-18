#!/usr/bin/env bash
#
# sync-version.sh — surgically update the version marker in the three config
# files that track the release version:
#
#   - package.json                  →   "version": "X.Y.Z"
#   - src-tauri/tauri.conf.json     →   "version": "X.Y.Z"
#   - src-tauri/Cargo.toml          →   version = "X.Y.Z"
#
# Why surgical sed (and NOT JSON.parse / jq / a full rewrite)?
#   package.json and tauri.conf.json both contain intentionally-compact arrays
#   (e.g. "targets": ["nsis", "msi", "deb", "appimage", "dmg"]). Re-parsing and
#   re-serializing with `JSON.stringify(..., null, 2)` or `jq` expands those
#   arrays onto multiple lines, producing a noisy diff unrelated to the version.
#   This script replaces ONLY the single version line in each file and leaves
#   every other byte untouched.
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

cd "$ROOT_DIR"

update_json "package.json"
update_json "src-tauri/tauri.conf.json"
update_cargo "src-tauri/Cargo.toml"

echo "Synced version to $VERSION across package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml"