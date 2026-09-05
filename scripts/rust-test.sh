#!/usr/bin/env bash
# rust-test.sh - fast/slow Rust test runner (contracts: docs/CLAUDE.md
# §Testing and src-tauri/tests/AGENTS.md).
#
# Modes:
#   (default)       fast suite: `cargo test` (slow tests are #[ignore = "slow"])
#   --full          fast + all slow tests (env knobs make them quick)
#   --changed [base] fast + slow tests whose areas changed vs base (default: main)
#   --live          citation_chaser live tests (Chrome + network)
#
# Slow tests run with TEST-ONLY env knobs (debug builds only):
#   BANGO_TEST_BACKOFF_MS=0       - skip retry backoff sleeps
#   BANGO_TEST_PBKDF2_ITERATIONS=1000 - cheap PBKDF2 in tests
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/tests/slow-manifest.toml"

MODE=fast
BASE=main
LIVE=0
EXTRA=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --full)
      MODE=full
      shift
      ;;
    --changed)
      MODE=changed
      shift
      if [[ $# -gt 0 && $1 != -* ]]; then
        BASE="$1"
        shift
      fi
      ;;
    --live)
      LIVE=1
      shift
      ;;
    -h | --help)
      awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
      exit 0
      ;;
    --)
      shift
      EXTRA+=("$@")
      break
      ;;
    *)
      EXTRA+=("$1")
      shift
      ;;
  esac
done

cd "$ROOT/src-tauri"

# Extract `area|paths|tests` lines from the TOML manifest.
read_manifest() {
  awk '
    /^\[\[area\]\]/ { if (name != "") flush() }
    /^name =/ { gsub(/[" ]/, "", $3); name = $3 }
    /^paths =/ { gsub(/[][",]/, ""); for (i = 3; i <= NF; i++) paths = paths " " $i }
    /^tests = \[/ { in_tests = 1; next }
    in_tests {
      if ($0 ~ /\]/) { in_tests = 0; sub(/\].*/, "") }
      gsub(/[][",]/, "")
      for (i = 1; i <= NF; i++) if ($i != "") tests = tests (tests == "" ? "" : ",") $i
    }
    END { if (name != "") flush() }
    function flush() { print name "|" paths "|" tests; paths = ""; tests = "" }
  ' "$MANIFEST"
}

# Run one area's slow tests with the TEST-ONLY env knobs.
run_slow_area() {
  local area="$1" tests="$2" t
  local -a names
  IFS=',' read -r -a names <<<"$tests"
  echo "[rust-test] slow: $area (${#names[@]} test(s))"
  BANGO_TEST_BACKOFF_MS=0 BANGO_TEST_PBKDF2_ITERATIONS=1000 \
    cargo test --test "$area" -- --ignored "${names[@]}" < /dev/null
}

# Does path $1 match any of the space-separated globs in $2?
# `dir/**` matches the dir itself plus everything under it; `stem*` matches
# any path beginning with `stem` (so `src/models/llm_config*` catches
# `src/models/llm_config.rs` and any `llm_config_*.rs` siblings).
# Globbing is disabled while iterating so manifest globs stay literal even
# when matching paths exist relative to the cwd.
path_matches_area() {
  local p="$1" glob dir stem found=1
  set -f
  for glob in $2; do
    if [[ "$glob" == *'/**' ]]; then
      dir="${glob%'/**'}"
      dir="${dir%/}"
      if [[ "$p" == "$dir" || "$p" == "$dir"/* ]]; then
        found=0
        break
      fi
    elif [[ "$glob" == *'*' ]]; then
      stem="${glob%'*'}"
      if [[ "$p" == "$stem"* ]]; then
        found=0
        break
      fi
    elif [[ "$p" == "$glob" ]]; then
      found=0
      break
    fi
  done
  set +f
  return "$found"
}

# ── fast suite (always runs first) ──────────────────────────────────
echo "[rust-test] fast: cargo test ${EXTRA[*]:-}"
cargo test "${EXTRA[@]}"

# ── slow tests per mode ─────────────────────────────────────────────
if [[ "$MODE" == full ]]; then
  while IFS='|' read -r area paths tests; do
    [[ -n "$tests" ]] && run_slow_area "$area" "$tests"
  done < <(read_manifest)
fi

if [[ "$MODE" == changed ]]; then
  # Resolve the base ref (accept branch names, origin/<branch>, or SHAs).
  if ! git rev-parse --verify "$BASE" >/dev/null 2>&1 \
    && ! git rev-parse --verify "origin/$BASE" >/dev/null 2>&1; then
    echo "[rust-test] base ref '$BASE' not found; falling back to HEAD~20" >&2
    BASE="HEAD~20"
  elif ! git rev-parse --verify "$BASE" >/dev/null 2>&1; then
    BASE="origin/$BASE"
  fi

  changed="$(git diff --name-only "$BASE"...HEAD; git diff --name-only; git diff --cached --name-only; git ls-files --others --exclude-standard)"
  changed="$(printf '%s\n' "$changed" | sort -u)"

  if printf '%s\n' "$changed" | grep -qE '^src-tauri/Cargo\.(toml|lock)$|^src-tauri/build\.rs$'; then
    echo "[rust-test] changed: build files touched - running all slow tests"
    while IFS='|' read -r area paths tests; do
      [[ -n "$tests" ]] && run_slow_area "$area" "$tests"
    done < <(read_manifest)
  else
    while IFS='|' read -r area paths tests; do
      [[ -z "$tests" ]] && continue
      while IFS= read -r p; do
        [[ -z "$p" ]] && continue
        if [[ "$p" == src-tauri/tests/"$area"/* || "$p" == src-tauri/tests/slow-manifest.toml ]] \
          || { [[ "$p" == src-tauri/src/* ]] && path_matches_area "${p#src-tauri/}" "$paths"; }; then
          echo "[rust-test] changed: $p -> area '$area'"
          run_slow_area "$area" "$tests"
          break
        fi
      done <<<"$changed"
    done < <(read_manifest)
  fi
fi

if [[ "$LIVE" == 1 ]]; then
  echo "[rust-test] live: citation_chaser tests (Chrome + network)"
  cargo test --test scraping -- --ignored citation_chaser_test < /dev/null
fi

echo "[rust-test] done"
