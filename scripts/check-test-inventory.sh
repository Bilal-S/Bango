#!/usr/bin/env bash
# check-test-inventory.sh - §5.0 Mechanism B: machine-checkable Test Inventory.
#
# Parses the binding `file::function` test identifiers from the fenced inventory
# tables in `.worktrees/chunkingplan.md` AND `.worktrees/tier4-plan.md`, and
# greps the named test files to confirm each listed test exists. Fails (exit 1)
# if any listed test is missing from its file.
#
# Wired into `npm run check:all` via the `check:test-inventory` npm script so
# the inventory is enforced at PR time.
#
# The inventory row format (machine-parseable):
#   | `src-tauri/tests/<file>.rs::<test_fn_name>` | human-readable assertion |
#   | `src/__tests__/<file>.test.ts::<test_fn_name>` | human-readable assertion |
#
# This script extracts the first backticked token, splits on `::`, and greps
# the file for the function/`it(` name.

set -euo pipefail

PLAN_DOCS=(docs/test-plans/language-plan-v2-tests.md docs/test-plans/translation-3-tests.md docs/test-plans/search-strategy-tests.md)
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Collect existing plan docs (non-fatal if any are absent, e.g. fresh checkout
# without docs). At least one must exist for the inventory to be checked.
EXISTING_PLAN_DOCS=()
for doc in "${PLAN_DOCS[@]}"; do
  if [[ -f "$doc" ]]; then
    EXISTING_PLAN_DOCS+=("$doc")
  fi
done

if [[ ${#EXISTING_PLAN_DOCS[@]} -eq 0 ]]; then
  echo "[check-test-inventory] no plan docs found in .worktrees/" >&2
  exit 0 # non-fatal (fresh checkout without .worktrees)
fi

# Extract inventory rows: lines matching `| `<path>::<name>` |`.
# We look for the §T3.7 + §T2.x + §T1.x + §T4.x inventory tables (any backticked
# file::function token in a table row). Parse ALL existing plan docs so the
# Tier 4 inventories in tier4-plan.md are enforced alongside the Tier 1-3
# inventories in chunkingplan.md.
mapfile -t ROWS < <(
  grep -hoE '`(src-tauri/(src|tests)/[^`]+|src/__tests__/[^`]+)::[a-zA-Z0-9_]+`' \
    "${EXISTING_PLAN_DOCS[@]}" \
    | tr -d '`' \
    | sort -u
)

if [[ ${#ROWS[@]} -eq 0 ]]; then
  echo "[check-test-inventory] no inventory rows found in ${EXISTING_PLAN_DOCS[*]}"
  exit 0
fi

missing=0
checked=0
for row in "${ROWS[@]}"; do
  file="${row%%::*}"
  fn="${row##*::}"
  checked=$((checked + 1))
  if [[ ! -f "$file" ]]; then
    echo "[check-test-inventory] MISSING FILE: ${row} (file does not exist on disk)" >&2
    missing=$((missing + 1))
    continue
  fi
  # Rust: `fn <name>(` or `fn <name> ()` or `#[ignore]`-stub `fn <name>(`.
  # TS: `it('<name>'` / `it("<name>"` (also matches the `.skip` stub variant
  # `it.skip('<name>'` used by the two-PR prep-PR protocol).
  if grep -qE "(fn ${fn}[ (]|(it|test)(\.skip)?\(['\"]${fn}['\"])" "$file"; then
    continue
  fi
  echo "[check-test-inventory] MISSING: ${row} (file exists but test not found)" >&2
  missing=$((missing + 1))
done

echo "[check-test-inventory] checked $checked inventory rows across $(echo "${ROWS[@]}" | tr ' ' '\n' | sed 's/::.*//' | sort -u | wc -l) files; $missing missing"

if [[ $missing -gt 0 ]]; then
  echo "[check-test-inventory] FAIL: $missing binding inventory tests are missing." >&2
  exit 1
fi

echo "[check-test-inventory] OK: all listed tests present."
exit 0