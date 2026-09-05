#!/usr/bin/env bash
# check-slow-manifest.sh - verifies `src-tauri/tests/slow-manifest.toml`
# stays in sync with the `#[ignore = "slow"]` tags in the test files.
#
# Checks:
#   1. Every `#[ignore = "slow"]` test is listed in the manifest.
#   2. Every manifest test exists, lives in the listed area, and carries the tag.
#   3. Every manifest area has a `tests/<area>/main.rs` binary.
#
# Wired into `npm run check:all` via the `check:slow-manifest` npm script.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TESTS_DIR="$ROOT/src-tauri/tests"
MANIFEST="$TESTS_DIR/slow-manifest.toml"
FAILED=0

# Tagged tests: scan each area's test files for the tag + the fn that follows it.
tagged="$(mktemp)"
manifest_tests="$(mktemp)"
while IFS= read -r f; do
  module="$(basename "$f" .rs)"
  area="$(basename "$(dirname "$f")")"
  awk -v m="$module" '
    /#\[ignore = "slow"\]/ { pending = 1; next }
    pending && /^[[:space:]]*(pub )?(async )?fn / {
      sub(/^.*fn /, ""); sub(/\(.*/, "");
      print m "::" $0
      pending = 0
    }
  ' "$f" >>"$tagged"
done < <(find "$TESTS_DIR" -name '*.rs' ! -name 'main.rs' -type f)
sort -u -o "$tagged" "$tagged"

# Manifest tests: `area|paths|tests` extraction mirrors scripts/rust-test.sh.
awk '
  /^\[\[area\]\]/ { if (name != "") flush() }
  /^name =/ { gsub(/[" ]/, "", $3); name = $3 }
  /^tests = \[/ { in_tests = 1; next }
  in_tests {
    if ($0 ~ /\]/) { in_tests = 0; sub(/\].*/, "") }
    gsub(/[][",]/, "")
    for (i = 1; i <= NF; i++) if ($i != "") print $i
  }
  END { if (name != "") flush() }
  function flush() { name = "" }
' "$MANIFEST" | sort -u >"$manifest_tests"

# 1. Tagged but missing from the manifest.
while IFS= read -r t; do
  [[ -z "$t" ]] && continue
  if ! grep -qxF "$t" "$manifest_tests"; then
    echo "[check-slow-manifest] tagged #[ignore = \"slow\"] but missing from manifest: $t" >&2
    FAILED=1
  fi
done <"$tagged"

# 2 + 3. Manifest entries must exist, be tagged, and live in the listed area.
area=""
while IFS= read -r line; do
  if [[ "$line" =~ ^\[\[area\]\] ]]; then
    area=""
  elif [[ "$line" =~ ^name\ =\ \"([a-z_0-9]+)\" ]]; then
    area="${BASH_REMATCH[1]}"
    if [[ ! -f "$TESTS_DIR/$area/main.rs" ]]; then
      echo "[check-slow-manifest] area '$area' has no tests/$area/main.rs binary" >&2
      FAILED=1
    fi
  elif [[ "$line" =~ \"([a-z_0-9]+)::([a-zA-Z_0-9]+)\" ]]; then
    module="${BASH_REMATCH[1]}"
    fn="${BASH_REMATCH[2]}"
    t="${module}::${fn}"
    [[ -n "$area" ]] || { echo "[check-slow-manifest] test outside area: $t" >&2; FAILED=1; continue; }
    f="$TESTS_DIR/$area/$module.rs"
    if [[ ! -f "$f" ]]; then
      echo "[check-slow-manifest] manifest test file missing: $t (expected $f)" >&2
      FAILED=1
      continue
    fi
    if ! grep -qxF "$t" "$tagged"; then
      echo "[check-slow-manifest] manifest test not tagged #[ignore = \"slow\"]: $t" >&2
      FAILED=1
    fi
  fi
done <"$MANIFEST"

rm -f "$tagged" "$manifest_tests"
if [[ "$FAILED" == 1 ]]; then
  echo "[check-slow-manifest] manifest out of sync (see above)" >&2
  exit 1
fi
echo "[check-slow-manifest] ok"
