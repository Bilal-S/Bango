#!/usr/bin/env bash
# Enforce Conventional Commits format on commit messages.
# Wired as a `commit-msg` git hook via `simple-git-hooks`.
#
# Format: type(scope): description
#   - type:  feat | fix | chore | docs | style | refactor | perf | test | build | ci | revert
#   - scope: optional, in parentheses
#   - description: non-empty
#
# Merge commits and revert-of-revert auto-commits start with `Merge ` or
# `Revert ` and are exempt.
#
# Example valid messages:
#   feat(bibliometrics): add co-citation network view
#   fix: handle poisoned mutex in lib.rs setup
#   chore(release): sync version to 2.5.1
#   docs: update AGENTS.md wiki section
set -euo pipefail

commit_msg_file="$1"
first_line=$(head -n 1 "$commit_msg_file")

# Exempt auto-generated commits.
case "$first_line" in
  'Merge '*|'Revert '*)
    exit 0
    ;;
esac

# Regex: optional scope, colon, space, non-empty description.
pattern='^(feat|fix|chore|docs|style|refactor|perf|test|build|ci|revert)(\([^)]+\))?: .+'

if ! [[ "$first_line" =~ $pattern ]]; then
  echo "ERROR: commit message does not follow Conventional Commits format." >&2
  echo "" >&2
  echo "  First line: $first_line" >&2
  echo "" >&2
  echo "  Expected format: type(scope): description" >&2
  echo "  Allowed types:  feat fix chore docs style refactor perf test build ci revert" >&2
  echo "  Scope is optional; description must be non-empty." >&2
  echo "" >&2
  echo "  Examples:" >&2
  echo "    feat(bibliometrics): add co-citation network view" >&2
  echo "    fix: handle poisoned mutex in lib.rs setup" >&2
  echo "    chore(release): sync version to 2.5.1" >&2
  exit 1
fi