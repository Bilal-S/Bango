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
#      authorizations in Zotero Settings -> Advanced. The probe SAVES
#      remember:true keys to $KEYFILE (key + server id, mode 600) and
#      reuses them on later runs while the live server id matches (the
#      app's decide_write_auth policy); any 401 write rejection clears the
#      file so the next run shows the dialog again.
#   3. POST /api/users/0/items + Zotero-API-Key + Zotero-Server-ID -> 200
#      with the successful/success/unchanged/failed envelope.
#   4. DELETE /api/users/0/items/<key> additionally needs
#      If-Unmodified-Since-Version: <item version> (else 428).
#
# Upload contract exercised by --upload (mirrors write_client.rs
# upload_file; recorded findings live in .worktrees/zotero3.md):
#   5. Attachment child item {"itemType":"attachment","parentItem":<key>,
#      "linkMode":"imported_file","contentType":"text/plain",
#      "title"/"filename": "{Author} - {<=30 title chars}.{ext}"} via the
#      batched new-item POST with a fresh Zotero-Write-Token (the app's
#      build_attachment_title convention; the probe item also targets the
#      UI-selected collection or COLLECTION).
#   6. Phase 1: POST /api/users/0/items/<attachmentKey>/file with urlencoded
#      md5/filename/filesize/mtime + "If-None-Match: *"
#      -> {"url","uploadKey"} or {"exists":1}.
#   7. Phase 2: POST the file bytes to <url> with
#      Content-Type: application/x-zotero-file -> 201.
#   8. Phase 3: POST the file endpoint again with urlencoded
#      upload=<uploadKey> -> 204; GET .../file then serves the upload.
#      (Verified live 2026-09-05 on Zotero 10.0.1: phase 1 answers 200 with a
#      LOCAL upload url <base>/api/local/uploads/<uploadKey> plus extra
#      contentType/prefix/suffix fields; bytes -> 201; register -> 204; the
#      file check GET -> 302 with a file:// Location into Zotero storage.)
#
# Usage:
#   scripts/zotero_write_probe.sh             # read-only probes (stages 1-3)
#   scripts/zotero_write_probe.sh --write     # stages 4-7: keyless 401 probe,
#                                             # authorize (dialog!), one item
#                                             # create, versioned delete
#   scripts/zotero_write_probe.sh --upload    # stages 4-5 + 8-13: authorize
#                                             # (dialog!), probe parent +
#                                             # attachment item, 3-phase file
#                                             # upload of a tiny .txt + file
#                                             # check + read-back verify.
#                                             # The parent item targets the
#                                             # UI-selected collection (or
#                                             # COLLECTION); items are KEPT
#                                             # in Zotero for inspection and
#                                             # their keys recorded in $STATE
#   scripts/zotero_write_probe.sh --cleanup   # stages 1-3 + 5 + deletes:
#                                             # authorize (dialog!), then
#                                             # versioned-delete every
#                                             # recorded probe item
#   scripts/zotero_write_probe.sh --meta      # stages 1-5 + metadata
#                                             # round-trip: creates the
#                                             # probe items (full-metadata +
#                                             # year-only + month-only date
#                                             # variants) and two child notes
#                                             # on the full item, reads every
#                                             # item back unauthenticated,
#                                             # and jq-asserts every metadata
#                                             # field, tag, parsedDate, and
#                                             # note (PASS/FAIL summary).
#                                             # Items are KEPT for inspection
#                                             # (keys recorded in $STATE);
#                                             # remove with --cleanup.
# Env: ZOTERO_BASE, WRITE_TIMEOUT (default 180 s dialog wait),
#      APP (authorize appName, default "Bango"), COLLECTION (target
#      collection key override for --upload; default: the UI-selected
#      collection via an exact-name match, else Unfiled), STATE (probe-item
#      key file, default ${TMPDIR:-/tmp}/zotero_write_probe_state), KEYFILE
#      (saved write key + server id, default
#      ${TMPDIR:-/tmp}/zotero_write_probe_key; delete it to force a fresh
#      authorize dialog). Needs curl, jq, md5sum, GNU stat.
# Exit codes: 0 ok or informational probe finding, 1 connector unreachable,
#      2 local API disabled or unexpected read failure, 3 script bug,
#      4 upload contract deviation or failed cleanup (stage output names
#      the failing phase).

set -euo pipefail

BASE="${ZOTERO_BASE:-http://localhost:23119}"
API="$BASE/api"
APP="${APP:-Bango}"
HDR="$(mktemp)"
BODY="$(mktemp)"
PAYLOAD="$(mktemp)"
FILEBYTES="$(mktemp)"
REMAIN="$(mktemp)"
STATE="${STATE:-${TMPDIR:-/tmp}/zotero_write_probe_state}"
KEYFILE="${KEYFILE:-${TMPDIR:-/tmp}/zotero_write_probe_key}"
trap 'command -v cleanup_probe_items >/dev/null 2>&1 && cleanup_probe_items; rm -f "$HDR" "$BODY" "$PAYLOAD" "$FILEBYTES" "$REMAIN"' EXIT

STATUS=""
SERVER_ID=""
KEY=""
ITEM_KEY=""
PARENT_KEY=""
ATTACH_KEY=""
UPLOAD_URL=""
UPLOAD_KEY=""
SKIP_UPLOAD=0
COLL_KEY=""
COLL_NAME=""

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

# write_token - fresh 32-char idempotency token (the app's build_write_token
# charset: no I, O, 0, 1, or l).
write_token() {
  local out=""
  while [ "${#out}" -lt 32 ]; do
    out="${out}$(tr -dc 'A-HJ-NP-Za-km-z2-9' </dev/urandom | head -c 64 || true)"
  done
  printf '%.32s' "$out"
}

# delete_item KEY - best-effort versioned delete (GET the item version, then
# DELETE with If-Unmodified-Since-Version). Sets STATUS; a missing item maps
# to STATUS=404-gone, an unreadable one to STATUS=no-version.
delete_item() {
  local key="$1" version=""
  req GET "$API/users/0/items/$key" 10 -H "Zotero-API-Version: 3"
  if [ "$STATUS" = "404" ]; then
    STATUS="404-gone"
    return 0
  fi
  version="$(jq -r '.version // empty' "$BODY" 2>/dev/null || true)"
  if [ -z "$version" ]; then
    STATUS="no-version"
    return 0
  fi
  req DELETE "$API/users/0/items/$key" 15 \
    -H "Zotero-API-Version: 3" -H "Zotero-Server-ID: $SERVER_ID" \
    -H "Zotero-API-Key: $KEY" \
    -H "If-Unmodified-Since-Version: $version"
}

# cleanup_probe_items - trap hook: remove probe items a failed run leaves in
# the real library (--write's item only; --upload/--meta keep their items on
# purpose - they stay recorded in $STATE until --cleanup runs).
cleanup_probe_items() {
  if [ "${MODE:-}" = "--upload" ] || [ "${MODE:-}" = "--meta" ]; then
    return 0
  fi
  [ -n "$KEY" ] && [ -n "$SERVER_ID" ] || return 0
  local key
  for key in "$ATTACH_KEY" "$PARENT_KEY" "$ITEM_KEY"; do
    [ -n "$key" ] || continue
    delete_item "$key" || true
  done
}

# record_key KEY - append a kept probe item key to $STATE for --cleanup.
record_key() {
  printf '%s\n' "$1" >> "$STATE"
}

# title_for_upload AUTHOR TITLE EXT - the app's build_attachment_title rule:
# "{Author} - {title cut at the last word boundary within 30 chars}.{ext}".
title_for_upload() {
  local author="$1" title="$2" ext="${3#.}"
  local cut="$title"
  if [ "${#title}" -gt 30 ]; then
    cut="${title:0:30}"
    cut="${cut% *}"
  fi
  if [ -n "$author" ]; then
    printf '%s - %s.%s' "$author" "$cut" "$ext"
  else
    printf '%s.%s' "$cut" "$ext"
  fi
}

# resolve_collection - pick the --upload target collection: COLLECTION env
# override, else the exact-name match of the Zotero UI selection (connector
# getSelectedCollection, like the app's export panel default: the response's
# numeric tree id is unreliable, the name vs libraryName pair is not).
# Sets COLL_KEY/COLL_NAME; both stay empty -> the probe item is Unfiled.
# The connector endpoint needs Content-Length (curl: --data-binary '{}');
# a body-less POST is rejected with 400, a form content-type with 400.
resolve_collection() {
  if [ -n "${COLLECTION:-}" ]; then
    COLL_KEY="$COLLECTION"
    req GET "$API/users/0/collections" 10 -H "Zotero-API-Version: 3"
    COLL_NAME="$(jq -r --arg k "$COLLECTION" '[.[] | select(.key == $k)][0].data.name // "unknown"' "$BODY" 2>/dev/null || true)"
    return 0
  fi
  req POST "$BASE/connector/getSelectedCollection" 5 \
    -H "X-Zotero-Connector-API-Version: 3" \
    -H "Content-Type: application/json" \
    --data-binary '{}'
  local selected="" library=""
  selected="$(jq -r '.name // empty' "$BODY" 2>/dev/null || true)"
  library="$(jq -r '.libraryName // empty' "$BODY" 2>/dev/null || true)"
  if [ -z "$selected" ] || [ "$selected" = "$library" ]; then
    echo "UI selection is the library root (${library:-unknown}) - leaving Unfiled"
    return 0
  fi
  req GET "$API/users/0/collections" 10 -H "Zotero-API-Version: 3"
  local hits=""
  hits="$(jq -r --arg n "$selected" '[.[] | select(.data.name == $n)] | length' "$BODY" 2>/dev/null || true)"
  if [ "$hits" = "1" ]; then
    COLL_KEY="$(jq -r --arg n "$selected" '[.[] | select(.data.name == $n)][0].key' "$BODY" 2>/dev/null || true)"
    COLL_NAME="$selected"
  else
    echo "selected collection '$selected' has $hits exact-name matches - leaving Unfiled"
  fi
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

MODE="${1:-}"
case "$MODE" in
  --write|--upload|--cleanup|--meta) ;;
  *)
    say "Done (read-only). Re-run with --write to probe the write contract,"
    echo "with --upload to create + upload probe items (kept for inspection),"
    echo "with --meta to round-trip + validate full metadata, dates, and notes,"
    echo "or with --cleanup to delete the kept probe items."
    exit 0
    ;;
esac

if [ "$MODE" != "--cleanup" ]; then
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
fi

# key_expired - clear the saved write key after a 401 write rejection so
# the next run re-authorizes (single-use or revoked key).
key_expired() {
  rm -f "$KEYFILE"
  echo "stored write key rejected (401) - cleared; re-run to authorize again"
}

say "Stage 5: authorize - CLICK Allow in Zotero"
# Write-key reuse policy (the app's decide_write_auth): a remember:true key
# saved in $KEYFILE is reused while the live server id matches; otherwise
# the authorize dialog runs once and the key is saved for next time.
STORED_KEY=""
STORED_SID=""
if [ -f "$KEYFILE" ]; then
  STORED_KEY="$(sed -n 1p "$KEYFILE")"
  STORED_SID="$(sed -n 2p "$KEYFILE")"
fi
if [ -n "$STORED_KEY" ] && [ "$STORED_SID" = "$SERVER_ID" ]; then
  KEY="$STORED_KEY"
  say "Stage 5: write authorization - reusing saved key (server id match)"
  echo "saved key file: $KEYFILE (delete it to force a fresh dialog)"
else
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
  if [ "$REMEMBER" = "true" ]; then
    printf '%s\n%s\n' "$KEY" "$SERVER_ID" > "$KEYFILE"
    chmod 600 "$KEYFILE"
    echo "key saved for reuse in $KEYFILE"
  else
    echo "remember=false - key is single-use, not saved"
  fi
fi

if [ "$MODE" = "--cleanup" ]; then
  say "Cleanup: versioned deletes of every recorded probe item"
  echo "state file: $STATE"
  if [ ! -f "$STATE" ]; then
    echo "no state file - nothing to clean up."
    exit 0
  fi
  : > "$REMAIN"
  while IFS= read -r key; do
    case "$key" in ''|'#'*) continue ;; esac
    delete_item "$key"
    echo "delete $key -> $STATUS"
    case "$STATUS" in
      204|404-gone) ;;
      *) printf '%s\n' "$key" >> "$REMAIN" ;;
    esac
  done < "$STATE"
  if [ -s "$REMAIN" ]; then
    mv "$REMAIN" "$STATE"
    echo "RESULT: some probe items could not be deleted - state kept at $STATE."
    exit 4
  fi
  rm -f "$REMAIN" "$STATE"
  echo "RESULT: all recorded probe items deleted. Library left clean."
  exit 0
fi

if [ "$MODE" = "--write" ]; then
  say "Stage 6: authenticated write (POST one probe item with the key)"
  cat > "$PAYLOAD" <<'JSON'
[{"itemType":"journalArticle","title":"Bango write-probe (safe to delete)","creators":[{"creatorType":"author","name":"Bango Probe"}],"tags":[],"collections":[]}]
JSON
  req POST "$API/users/0/items" 20 \
    -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
    -H "Zotero-Server-ID: $SERVER_ID" -H "Zotero-API-Key: $KEY" \
    --data-binary "@$PAYLOAD"
  echo "status=$STATUS $(hdr Last-Modified-Version)"
  if [ "$STATUS" = "401" ]; then
    key_expired
    exit 0
  fi
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
    ITEM_KEY=""
    echo "RESULT: full write contract verified (authorize -> key in body ->"
    echo "authenticated POST -> versioned DELETE). Library left clean."
  fi
  exit 0
fi

# ── --meta: full metadata/date/tags/notes round-trip validation ──────────────
# check LABEL EXPECTED ACTUAL - one assertion; PASS/FAIL counters.
META_PASS=0
META_FAIL=0
check() {
  if [ "$2" = "$3" ]; then
    echo "  ok: $1"
    META_PASS=$((META_PASS + 1))
  else
    echo "  FAIL: $1"
    echo "        expected: $2"
    echo "        actual:   $3"
    META_FAIL=$((META_FAIL + 1))
  fi
}

if [ "$MODE" = "--meta" ]; then
  say "Stage 6 (--meta): create the validation items (server-assigned keys)"
  # Live-verified contract note: the LOCAL API rejects locally generated keys
  # on new-item POSTs (428 "Either If-Unmodified-Since-Version or 'version'
  # property must be provided for 'key'-based writes"), so the web-API
  # parent+child-in-one-batch trick does not apply here. The probe mirrors
  # the app's export flow: article batch first, then child notes referencing
  # the created parent key from the success envelope.
  # Exactly the field set the app's build_item_json emits for a full journal
  # article plus the two date variants build_export_date produces.
  cat > "$PAYLOAD" <<'JSON'
[
  {
    "itemType": "journalArticle",
    "title": "Bango meta-probe full item (safe to delete)",
    "creators": [
      {"creatorType": "author", "firstName": "Jane", "lastName": "Doe"},
      {"creatorType": "author", "firstName": "John", "lastName": "Smith"},
      {"creatorType": "author", "name": "World Health Organization"}
    ],
    "abstractNote": "Probe abstract.",
    "publicationTitle": "Journal of Probes",
    "volume": "7",
    "issue": "2",
    "pages": "10-20",
    "date": "2025-11-25",
    "DOI": "10.1/meta-probe",
    "url": "https://example.com/probe",
    "language": "en",
    "ISSN": "1234-5678",
    "extra": "Imported note text line",
    "tags": [{"tag": "machine-learning"}, {"tag": "Physics"}],
    "collections": [],
    "relations": {}
  },
  {
    "itemType": "journalArticle",
    "title": "Bango meta-probe year-only date (safe to delete)",
    "creators": [{"creatorType": "author", "name": "Bango Probe"}],
    "date": "2025",
    "DOI": "10.1/meta-probe-year",
    "tags": [],
    "collections": []
  },
  {
    "itemType": "journalArticle",
    "title": "Bango meta-probe month-only date (safe to delete)",
    "creators": [{"creatorType": "author", "name": "Bango Probe"}],
    "date": "2025-11",
    "DOI": "10.1/meta-probe-month",
    "tags": [],
    "collections": []
  }
]
JSON
  req POST "$API/users/0/items" 20 \
    -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
    -H "Zotero-Server-ID: $SERVER_ID" -H "Zotero-API-Key: $KEY" \
    -H "Zotero-Write-Token: $(write_token)" \
    --data-binary "@$PAYLOAD"
  echo "status=$STATUS $(hdr Last-Modified-Version)"
  if [ "$STATUS" = "401" ]; then
    key_expired
    exit 0
  fi
  if [ "$STATUS" != "200" ]; then
    echo "RESULT: item batch creation failed: $(head -c 400 "$BODY")"
    exit 4
  fi
  FULL_KEY="$(jq -r '.success["0"] // .successful["0"].key // empty' "$BODY")"
  YEAR_KEY="$(jq -r '.success["1"] // .successful["1"].key // empty' "$BODY")"
  MONTH_KEY="$(jq -r '.success["2"] // .successful["2"].key // empty' "$BODY")"
  echo "created keys: full=$FULL_KEY year=$YEAR_KEY month=$MONTH_KEY"
  if [ -z "$FULL_KEY" ] || [ -z "$YEAR_KEY" ] || [ -z "$MONTH_KEY" ]; then
    echo "RESULT: envelope did not report all three keys: $(head -c 400 "$BODY")"
    exit 4
  fi

  say "Stage 6b (--meta): create the child notes on the full item"
  # The two notes build_note_item_json produces for a split user-note block.
  cat > "$PAYLOAD" <<JSON
[
  {
    "itemType": "note",
    "parentItem": "$FULL_KEY",
    "note": "First probe note<br/>line two",
    "tags": []
  },
  {
    "itemType": "note",
    "parentItem": "$FULL_KEY",
    "note": "Second probe note",
    "tags": []
  }
]
JSON
  req POST "$API/users/0/items" 20 \
    -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
    -H "Zotero-Server-ID: $SERVER_ID" -H "Zotero-API-Key: $KEY" \
    -H "Zotero-Write-Token: $(write_token)" \
    --data-binary "@$PAYLOAD"
  echo "status=$STATUS $(hdr Last-Modified-Version)"
  if [ "$STATUS" = "401" ]; then
    key_expired
    exit 0
  fi
  if [ "$STATUS" != "200" ]; then
    echo "RESULT: note batch creation failed: $(head -c 400 "$BODY")"
    exit 4
  fi
  NOTE1_KEY="$(jq -r '.success["0"] // .successful["0"].key // empty' "$BODY")"
  NOTE2_KEY="$(jq -r '.success["1"] // .successful["1"].key // empty' "$BODY")"
  echo "created note keys: $NOTE1_KEY $NOTE2_KEY"

  say "Stage 7 (--meta): read-back verify - full metadata item"
  req GET "$API/users/0/items/$FULL_KEY" 10 -H "Zotero-API-Version: 3"
  if [ "$STATUS" != "200" ]; then
    echo "RESULT: read-back failed for $FULL_KEY ($STATUS)."
    exit 4
  fi
  check "itemType"            "journalArticle"                       "$(jq -r '.data.itemType' "$BODY")"
  check "title"               "Bango meta-probe full item (safe to delete)" "$(jq -r '.data.title' "$BODY")"
  check "abstractNote"        "Probe abstract."                      "$(jq -r '.data.abstractNote' "$BODY")"
  check "publicationTitle"    "Journal of Probes"                    "$(jq -r '.data.publicationTitle' "$BODY")"
  check "volume"              "7"                                    "$(jq -r '.data.volume' "$BODY")"
  check "issue"               "2"                                    "$(jq -r '.data.issue' "$BODY")"
  check "pages"               "10-20"                                "$(jq -r '.data.pages' "$BODY")"
  check "date (full ISO)"     "2025-11-25"                           "$(jq -r '.data.date' "$BODY")"
  check "parsedDate (full)"   "starts with 2025-11-25"               "$(jq -r 'if (.meta.parsedDate // "" | startswith("2025-11-25")) then "starts with 2025-11-25" else .meta.parsedDate end' "$BODY")"
  check "DOI"                 "10.1/meta-probe"                      "$(jq -r '.data.DOI' "$BODY")"
  check "url"                 "https://example.com/probe"            "$(jq -r '.data.url' "$BODY")"
  check "language"            "en"                                   "$(jq -r '.data.language' "$BODY")"
  check "ISSN"                "1234-5678"                            "$(jq -r '.data.ISSN' "$BODY")"
  check "extra"               "Imported note text line"              "$(jq -r '.data.extra' "$BODY")"
  check "creators (count)"    "3"                                    "$(jq -r '.data.creators | length' "$BODY")"
  check "first author name"   "Doe"                                  "$(jq -r '.data.creators[0].lastName' "$BODY")"
  check "institutional author" "World Health Organization"            "$(jq -r '.data.creators[2].name' "$BODY")"
  check "tags (sorted set)"   "Physics,machine-learning"             "$(jq -r '[.data.tags[].tag] | sort | join(",")' "$BODY")"

  say "Stage 8 (--meta): read-back verify - date variants"
  req GET "$API/users/0/items/$YEAR_KEY" 10 -H "Zotero-API-Version: 3"
  check "year-only date"      "2025"                                 "$(jq -r '.data.date' "$BODY")"
  check "year-only parsedDate" "starts with 2025"                     "$(jq -r 'if (.meta.parsedDate // "" | startswith("2025")) then "starts with 2025" else .meta.parsedDate end' "$BODY")"
  req GET "$API/users/0/items/$MONTH_KEY" 10 -H "Zotero-API-Version: 3"
  check "month-only date"     "2025-11"                              "$(jq -r '.data.date' "$BODY")"
  check "month-only parsedDate" "starts with 2025-11"                 "$(jq -r 'if (.meta.parsedDate // "" | startswith("2025-11")) then "starts with 2025-11" else .meta.parsedDate end' "$BODY")"

  say "Stage 9 (--meta): read-back verify - child notes"
  req GET "$API/users/0/items/$FULL_KEY/children" 10 -H "Zotero-API-Version: 3"
  if [ "$STATUS" != "200" ]; then
    echo "RESULT: children fetch failed for $FULL_KEY ($STATUS)."
    exit 4
  fi
  check "child note count"    "2"                                    "$(jq -r '[.[] | select(.data.itemType == "note")] | length' "$BODY")"
  check "note 1 content"      "First probe note<br/>line two"        "$(jq -r '[.[] | select(.data.itemType == "note") | .data.note] | sort | .[0]' "$BODY")"
  check "note 2 content"      "Second probe note"                    "$(jq -r '[.[] | select(.data.itemType == "note") | .data.note] | sort | .[1]' "$BODY")"
  check "note parentItem"     "$FULL_KEY"                            "$(jq -r --arg k "$FULL_KEY" '[.[] | select(.data.itemType == "note") | .data.parentItem] | if (length > 0 and all(. == $k)) then $k else join(",") end' "$BODY")"

  say "Stage 10 (--meta): summary"
  record_key "$FULL_KEY"
  record_key "$YEAR_KEY"
  record_key "$MONTH_KEY"
  record_key "$NOTE1_KEY"
  record_key "$NOTE2_KEY"
  echo "assertions: $META_PASS passed, $META_FAIL failed"
  if [ "$META_FAIL" -gt 0 ]; then
    echo "RESULT: metadata round-trip FAILED ($META_FAIL assertions)."
    echo "Items kept for inspection (keys in $STATE): remove with:"
    echo "  scripts/zotero_write_probe.sh --cleanup"
    exit 4
  fi
  echo "RESULT: full metadata/date/tags/notes round-trip verified."
  echo "Items kept for inspection (keys in $STATE): remove with:"
  echo "  scripts/zotero_write_probe.sh --cleanup"
  exit 0
fi

resolve_collection
say "Stage 8: create probe parent item (batch POST with Zotero-Write-Token)"
if [ -n "$COLL_KEY" ]; then
  echo "target collection: $COLL_NAME ($COLL_KEY)"
  COLL_JSON="[\"$COLL_KEY\"]"
else
  echo "target collection: none - the item will land in Unfiled Items"
  COLL_JSON="[]"
fi
cat > "$PAYLOAD" <<JSON
[{"itemType":"journalArticle","title":"Bango upload-probe (safe to delete)","creators":[{"creatorType":"author","firstName":"Bango","lastName":"Probe"}],"tags":[],"collections":$COLL_JSON}]
JSON
req POST "$API/users/0/items" 20 \
  -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
  -H "Zotero-Server-ID: $SERVER_ID" -H "Zotero-API-Key: $KEY" \
  -H "Zotero-Write-Token: $(write_token)" \
  --data-binary "@$PAYLOAD"
echo "status=$STATUS $(hdr Last-Modified-Version)"
if [ "$STATUS" = "401" ]; then
  key_expired
  exit 4
fi
PARENT_KEY="$(jq -r '.success["0"] // .successful["0"].key // empty' "$BODY" 2>/dev/null || true)"
echo "created parent item: ${PARENT_KEY:-none}"
if [ -z "$PARENT_KEY" ]; then
  echo "RESULT: parent item creation failed: $(head -c 300 "$BODY")"
  exit 4
fi
record_key "$PARENT_KEY"

say "Stage 9: create attachment child item (imported_file, text/plain)"
FRIENDLY="$(title_for_upload "Probe" "Bango upload-probe (safe to delete)" "txt")"
echo "attachment title/filename: $FRIENDLY"
# Field order matters: linkMode MUST precede filename/contentType or Zotero
# rejects the item ("Link mode must be set before setting attachment path").
cat > "$PAYLOAD" <<JSON
[{"itemType":"attachment","parentItem":"$PARENT_KEY","linkMode":"imported_file","title":"$FRIENDLY","contentType":"text/plain","filename":"$FRIENDLY","tags":[]}]
JSON
req POST "$API/users/0/items" 20 \
  -H "Zotero-API-Version: 3" -H "Content-Type: application/json" \
  -H "Zotero-Server-ID: $SERVER_ID" -H "Zotero-API-Key: $KEY" \
  -H "Zotero-Write-Token: $(write_token)" \
  --data-binary "@$PAYLOAD"
echo "status=$STATUS"
if [ "$STATUS" = "401" ]; then
  key_expired
  exit 4
fi
ATTACH_KEY="$(jq -r '.success["0"] // .successful["0"].key // empty' "$BODY" 2>/dev/null || true)"
echo "created attachment item: ${ATTACH_KEY:-none}"
if [ -z "$ATTACH_KEY" ]; then
  echo "RESULT: attachment item creation failed: $(head -c 300 "$BODY")"
  exit 4
fi
record_key "$ATTACH_KEY"

say "Stage 10: upload phase 1 - auth form (md5/filename/filesize/mtime + If-None-Match: *)"
printf 'Bango Zotero local-API upload probe (safe to delete).\n' > "$FILEBYTES"
MD5="$(md5sum "$FILEBYTES" | cut -d' ' -f1)"
FSIZE="$(wc -c < "$FILEBYTES" | tr -d '[:space:]')"
MTIME_MS="$(( $(stat -c%Y "$FILEBYTES") * 1000 ))"
echo "probe file: $FRIENDLY size=${FSIZE}B md5=$MD5 mtime_ms=$MTIME_MS"
# Zotero's local form decoder passes '+' through literally (a '+ '-encoded
# space is stored as a literal '+' in the filename), so spaces are sent
# pre-encoded as %20 in a manually built urlencoded body.
FILENAME_ENC="$(printf '%s' "$FRIENDLY" | sed 's/%/%25/g; s/ /%20/g')"
FILE_URL="$API/users/0/items/$ATTACH_KEY/file"
req POST "$FILE_URL" 20 \
  -H "Zotero-API-Version: 3" \
  -H "Zotero-Server-ID: $SERVER_ID" \
  -H "Zotero-API-Key: $KEY" \
  -H "If-None-Match: *" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data-binary "md5=$MD5&filename=$FILENAME_ENC&filesize=$FSIZE&mtime=$MTIME_MS"
echo "status=$STATUS body: $(head -c 300 "$BODY")"
if [ "$STATUS" != "200" ]; then
  echo "RESULT: upload phase 1 (auth form $FILE_URL) failed with $STATUS."
  exit 4
fi
if [ "$(jq -r '.exists // empty' "$BODY" 2>/dev/null || true)" = "1" ]; then
  echo "phase 1 answered exists=1 (content already known) - skipping phases 2-3"
  SKIP_UPLOAD=1
else
  UPLOAD_URL="$(jq -r '.url // empty' "$BODY" 2>/dev/null || true)"
  UPLOAD_KEY="$(jq -r '.uploadKey // empty' "$BODY" 2>/dev/null || true)"
  echo "authorized upload: url=$UPLOAD_URL uploadKey=${UPLOAD_KEY:-none}"
  if [ -z "$UPLOAD_URL" ] || [ -z "$UPLOAD_KEY" ]; then
    echo "RESULT: unexpected phase 1 body (neither url/uploadKey nor exists=1)."
    exit 4
  fi
fi

if [ "$SKIP_UPLOAD" = "1" ]; then
  say "Stage 11-12: skipped (exists=1 short-circuit, no byte transfer)"
else
  say "Stage 11: upload phase 2 - bytes (Content-Type: application/x-zotero-file)"
  req POST "$UPLOAD_URL" 30 \
    -H "Zotero-Server-ID: $SERVER_ID" \
    -H "Zotero-API-Key: $KEY" \
    -H "Content-Type: application/x-zotero-file" \
    --data-binary "@$FILEBYTES"
  echo "status=$STATUS (expect 201)"
  if [ "$STATUS" != "201" ] && [ "$STATUS" != "200" ]; then
    echo "RESULT: upload phase 2 (bytes $UPLOAD_URL) failed with $STATUS: $(head -c 300 "$BODY")"
    exit 4
  fi

  say "Stage 12: upload phase 3 - register (upload=<uploadKey>), then file check"
  req POST "$FILE_URL" 20 \
    -H "Zotero-API-Version: 3" \
    -H "Zotero-Server-ID: $SERVER_ID" \
    -H "Zotero-API-Key: $KEY" \
    --data-urlencode "upload=$UPLOAD_KEY"
  echo "register status=$STATUS (expect 204)"
  if [ "$STATUS" != "204" ]; then
    echo "RESULT: upload phase 3 (register $FILE_URL) failed with $STATUS: $(head -c 300 "$BODY")"
    exit 4
  fi
  req GET "$FILE_URL" 10 -H "Zotero-API-Version: 3"
  echo "file check: status=$STATUS $(hdr Location)"
  if [ "$STATUS" != "302" ] && [ "$STATUS" != "200" ]; then
    echo "RESULT: uploaded file not served afterwards (GET -> $STATUS): $(head -c 200 "$BODY")"
    exit 4
  fi
fi

say "Stage 13: read-back verify (kept items, unauthenticated GET)"
req GET "$API/users/0/items/$PARENT_KEY" 10 -H "Zotero-API-Version: 3"
echo "parent $PARENT_KEY -> $STATUS: $(jq -r '.data.title // empty' "$BODY" 2>/dev/null || true)"
req GET "$API/users/0/items/$ATTACH_KEY" 10 -H "Zotero-API-Version: 3"
echo "attachment $ATTACH_KEY -> $STATUS: title='$(jq -r '.data.title // empty' "$BODY" 2>/dev/null || true)' file=$(jq -r '.data.filename // empty' "$BODY" 2>/dev/null || true) ($(jq -r '.data.linkMode // empty' "$BODY" 2>/dev/null || true))"
echo "RESULT: 3-phase upload verified. Items are KEPT in Zotero for inspection:"
echo "  parent item $PARENT_KEY ('Bango upload-probe')"
echo "  attachment  $ATTACH_KEY ($FRIENDLY; file check above)"
echo "Keys recorded in $STATE - remove them later with:"
echo "  scripts/zotero_write_probe.sh --cleanup"
