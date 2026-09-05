#!/usr/bin/env bash
# zotero_write_probe.sh - live probe of the Zotero local-API write
# authorization flow (contract verified live 2026-09-04 against Zotero 10.0.1;
# see .worktrees/zotero2.md Background for the recorded findings).
# Dev utility only - not part of the shipped app, not wired into check:all.
#
# Confirmed contract exercised by --write:
#   1. POST /api/users/0/items WITHOUT a key -> 401 + hint
#      "POST /api/local/authorize to obtain one" (also proves writes need the
#      Zotero-Server-ID request header; without it -> 428).
#   2. POST /api/local/authorize {"appName": APP} + Zotero-Server-ID header
#      -> blocks while a confirmation dialog shows in Zotero -> on Allow
#      200 {"key": <32 chars>, "remember": bool}. remember:false keys are
#      single-use; remember:true keys persist until the user clears write
#      authorizations in Zotero Settings -> Advanced.
#   3. POST /api/users/0/items + Zotero-API-Key + Zotero-Server-ID -> 200
#      with the successful/success/unchanged/failed envelope.
#   4. DELETE /api/users/0/items/<key> additionally needs
#      If-Unmodified-Since-Version: <item version> (else 428).
#
# Usage:
#   scripts/zotero_write_probe.sh           # read-only probes (stages 1-3)
#   scripts/zotero_write_probe.sh --write   # stages 4-7: keyless 401 probe,
#                                           # authorize (dialog!), one item
#                                           # create, versioned delete
# Env: ZOTERO_BASE, WRITE_TIMEOUT (default 180 s dialog wait),
#      APP (authorize appName, default "Bango").

set -euo pipefail

BASE="${ZOTERO_BASE:-http://localhost:23119}"
API="$BASE/api"
APP="${APP:-Bango}"
HDR="$(mktemp)"
BODY="$(mktemp)"
PAYLOAD="$(mktemp)"
trap 'rm -f "$HDR" "$BODY" "$PAYLOAD"' EXIT

STATUS=""
SERVER_ID=""

say() { printf '\n==== %s ====\n' "$1"; }

# req METHOD URL TIMEOUT [extra curl args...] - one request; captures STATUS,
# response headers in $HDR, response body in $BODY.
req() {
  local method="$1" url="$2" timeout="$3"
  shift 3
  STATUS="$(curl -sS -m "$timeout" -X "$method" -D "$HDR" -o "$BODY" \
    -w '%{http_code}' "$@" "$url" 2>/dev/null)" || STATUS="conn-failed"
}

# hdr NAME - first response header line (case-insensitive), CR stripped.
hdr() {
  { grep -i "^$1:" "$HDR" || true; } | head -1 | tr -d '\r'
}

say "Stage 1: connector ping"
req GET "$BASE/connector/ping" 5
VERSION="$(hdr X-Zotero-Version | cut -d' ' -f2-)"
echo "status=$STATUS version=${VERSION:-unknown}"
if [ "$STATUS" != "200" ]; then
  echo "Zotero connector server not reachable at $BASE"
  exit 1
fi
MAJOR="${VERSION%%.*}"
if [ "${MAJOR:-0}" -lt 10 ] 2>/dev/null; then
  echo "note: Zotero $VERSION detected - local API writes need Zotero 10+"
fi

say "Stage 2: local API enablement (GET /api/)"
req GET "$API/" 5
SERVER_ID="$(hdr Zotero-Server-ID | cut -d' ' -f2-)"
echo "status=$STATUS $(hdr Zotero-API-Version) server-id=${SERVER_ID:-none}"
if [ "$STATUS" = "403" ]; then
  echo "Local API is DISABLED. Enable it in Zotero:"
  echo '  Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero"'
  echo "then re-run this script."
  exit 2
fi
if [ "$STATUS" != "200" ]; then
  echo "unexpected status $STATUS"
  head -c 300 "$BODY"
  exit 2
fi

say "Stage 3: unauthenticated read (GET /api/users/0/collections)"
req GET "$API/users/0/collections" 10 -H "Zotero-API-Version: 3"
echo "status=$STATUS collections=$(jq 'length' "$BODY" 2>/dev/null || echo '?')"
echo "$(hdr Last-Modified-Version) server-id=${SERVER_ID:-none}"

if [ "${1:-}" != "--write" ]; then
  say "Done (read-only). Re-run with --write to probe key delivery."
  exit 0
fi

say "Stage 4: keyless write (POST /api/users/0/items, expect 401)"
cat > "$PAYLOAD" <<'JSON'
[{"itemType":"journalArticle","title":"Bango write-probe (safe to delete)","creators":[{"creatorType":"author","name":"Bango Probe"}],"tags":[],"collections":[]}]
JSON
req POST "$API/users/0/items" 20 \
  -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
  -H "Zotero-Server-ID: $SERVER_ID" \
  --data-binary "@$PAYLOAD"
echo "status=$STATUS body: $(head -c 200 "$BODY")"
if [ "$STATUS" = "428" ]; then
  echo "no Zotero-Server-ID header echoed (bug in this script?)"
  exit 3
fi
if [ "$STATUS" != "401" ]; then
  echo "expected 401 with the authorize hint - write may need no key on this build"
fi

say "Stage 5: authorize (POST /api/local/authorize) - CLICK Allow in Zotero"
printf '{"appName":"%s"}' "$APP" > "$PAYLOAD"
req POST "$API/local/authorize" "${WRITE_TIMEOUT:-180}" \
  -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
  -H "Zotero-Server-ID: $SERVER_ID" \
  --data-binary "@$PAYLOAD"
echo "status=$STATUS"
cat "$HDR"
echo "body: $(head -c 300 "$BODY")"
if [ "$STATUS" = "403" ]; then
  echo "RESULT: authorization denied by the user."
  exit 0
fi
KEY="$(jq -r '.key // empty' "$BODY" 2>/dev/null || true)"
REMEMBER="$(jq -r '.remember // empty' "$BODY" 2>/dev/null || true)"
if [ -z "$KEY" ]; then
  echo "RESULT: no key granted - see body above."
  exit 0
fi
echo "key granted (remember=$REMEMBER): $KEY"

say "Stage 6: authenticated write (POST one probe item with the key)"
cat > "$PAYLOAD" <<'JSON'
[{"itemType":"journalArticle","title":"Bango write-probe (safe to delete)","creators":[{"creatorType":"author","name":"Bango Probe"}],"tags":[],"collections":[]}]
JSON
req POST "$API/users/0/items" 20 \
  -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
  -H "Zotero-Server-ID: $SERVER_ID" -H "Zotero-API-Key: $KEY" \
  --data-binary "@$PAYLOAD"
echo "status=$STATUS $(hdr Last-Modified-Version)"
ITEM_KEY="$(jq -r '.success["0"] // .successful["0"].key // empty' "$BODY" 2>/dev/null || true)"
echo "created item: ${ITEM_KEY:-none}"
if [ -z "$ITEM_KEY" ]; then
  echo "RESULT: item creation failed: $(head -c 300 "$BODY")"
  exit 0
fi

say "Stage 7: cleanup (versioned DELETE with the same key)"
req GET "$API/users/0/items/$ITEM_KEY" 10 -H "Zotero-API-Version: 3"
ITEM_VERSION="$(jq -r '.version // empty' "$BODY" 2>/dev/null || true)"
req DELETE "$API/users/0/items/$ITEM_KEY" 15 \
  -H "Zotero-API-Version: 3" -H "Zotero-Server-ID: $SERVER_ID" \
  -H "Zotero-API-Key: $KEY" -H "If-Unmodified-Since-Version: $ITEM_VERSION"
echo "delete status=$STATUS (204 = removed; 428 = missing version header)"
if [ "$STATUS" = "204" ]; then
  echo "RESULT: full write contract verified (authorize -> key in body ->"
  echo "authenticated POST -> versioned DELETE). Library left clean."
fi
