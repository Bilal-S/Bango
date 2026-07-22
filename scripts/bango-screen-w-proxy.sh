#!/bin/bash
#
# Start Bango in dev mode with:
#   1. HTTP/HTTPS proxy on 127.0.0.1:8080 (for network-traffic inspection in
#      mitmproxy / Charles / Burp — useful for diagnosing slow LLM calls).
#   2. Screening diagnostics captured to a timestamped log file so the
#      `[screening:diag]` lines (phase transitions, heartbeat, cancel tracing,
#      slow-lock warnings, orchestrator timeouts) can be reviewed after a hang.
#
# Usage:
#   ./scripts/bango-screen-w-proxy.sh
#
# Then reproduce the hang, click Stop/Pause, wait ~30s, stop the app (Ctrl+C),
# and inspect the log:
#   grep '[screening:diag]' screening-YYYYMMDD-HHMMSS.log | tail -100
#
# The Rust stderr (where `eprintln!("[screening:diag] …")` lands) is tee'd to
# BOTH the terminal (so you still see live output) AND the log file. The Vite
# dev-server stdout stays on the terminal only (it's noisy and not relevant to
# the screening hang).
#
# Mirrors `dev-w-proxy.sh` at the repo root; this script lives under `scripts/`
# next to the other operational helpers.

set -euo pipefail

# ── Proxy (identical to dev-w-proxy.sh) ─────────────────────────────────────
export http_proxy=http://127.0.0.1:8080
export https_proxy=http://127.0.0.1:8080
export NO_PROXY=localhost,127.0.0.1

# ── Screening diagnostics log ───────────────────────────────────────────────
# Timestamped filename so each run gets its own file (no silent overwrites).
# Placed in the repo root so it's easy to find with `grep`; `*.log` is already
# in `.gitignore`, so the files won't be committed. The Rust process inherits
# this fd, so all child-process stderr is captured too.
LOG_FILE="screening-$(date +%Y%m%d-%H%M%S).log"

echo "Proxy set to 127.0.0.1:8080"
echo "Screening diagnostics log: $LOG_FILE"
echo "  (Rust stderr is tee'd to this file AND the terminal)"
echo "  Review with: grep '[screening:diag]' $LOG_FILE | tail -100"
echo "Starting Tauri dev server... (Ctrl+C to stop)"
echo

# Redirect Rust stderr to the log file while keeping it live on the terminal.
# Process substitution `>(tee … >&2)` duplicates stderr: one copy to the file,
# one copy back to the terminal's stderr. Stdout (Vite) is untouched.
# `exec` ensures signals (Ctrl+C) propagate to the dev server cleanly.
exec npm run tauri dev 2> >(tee "$LOG_FILE" >&2)