# zotero/

## Purpose

Zotero local API integration (spec 3.1.1 import + 10.2 export). Import
wizard source (connection check, recursive collection fetch, mapping to
`RisRecord`, bulk attachment discovery, 302-Location path resolution) and
the Zotero 10+ local write API client (key authorization, batched item
creation, 3-phase file upload, versioned deletes).

## Ownership

- Owns: `mod.rs` (serde types + `ZoteroError`), `client.rs` (read client +
  path resolution), `mapping.rs` (import-side pure helpers),
  `export_mapping.rs` (export-side pure helpers), `write_client.rs`
  (write API).
- Commands live in `commands/zotero.rs`; the DB phase of the import
  (`insert_articles_batch` -> classify -> journal links -> `ris_keyword`
  tags -> staleness) also lives there.
- Frontend consumers: `zotero-collection-picker.vue`, `use-zotero.ts`
  (import); `zotero-export-panel.vue`, `use-zotero-export.ts` (export);
  `types/zotero.ts`.

## Local Contracts

### Read client (`client.rs`)

One shared `OnceLock<reqwest::Client>`: 5-second total timeout (local JSON
only), `redirect::Policy::none()` (an attachment file endpoint's 302
`Location` is data, not something to follow), `Zotero-API-Version: 3`
default header. Total status mapping: refused/timeout -> `NotRunning`,
`403` -> `ApiDisabled`, `404` with the connector-server body "No endpoint found"
-> `ApiEndpointMissing` (the local API was not reachable at that moment -
startup race or preference not yet active; surfaces the actionable "Could not
communicate with Zotero..." guidance with a `(404)` suffix), any other
non-success/decode failure -> `Http`/`Parse` with the status + body snippet
(genuine API 404s like a missing collection keep their raw form). `/collections/{key}/items` is NOT
recursive (verified live on Zotero 10.0.1): `fetch_collection_items` walks
subcollections via `/collections/{key}/collections` with a seen-set and
partitions out child items; `fetch_collection_top_items` (`/items/top`) is
the non-recursive export diff baseline. Attachments come from ONE bulk
`/users/0/items?itemType=attachment` request (grouped by `data.parentItem`),
with a per-item `/children` fallback through a bounded pool of 4; child
notes come from the mirrored `fetch_all_notes` (`?itemType=note`).

### Path resolution (`client::resolve_attachment_path`)

Pure string logic over the 302 `Location` - no `Url::to_file_path()` - so
every branch (POSIX plain, percent-encoded, unicode, Windows drive letter,
UNC share, non-file scheme rejection) runs on every platform. The defensive
200-with-body case writes a temp file under `std::env::temp_dir()` (PDF
magic bytes pick the extension).

### Import mapping (`mapping.rs`)

Scholarly itemType -> RIS type table (unsupported types -> `None` -> skipped
+ "Unsupported Zotero item type" error group); creators prefer `author`,
fall back to `editor`, institutional `name` verbatim; `meta.parsedDate` ->
year; DOI through the canonical `ris::doi::normalize_doi`. Zotero tags are
deliberately NOT written to `keywords` - they flow to Bango tags post-insert
(source `ris_keyword`). `sanitize_zotero_tag` applies spec 2.1 naming rules.
Attachment candidacy: linkMode `imported_file`/`linked_file`/`imported_url`
(live Zotero 10 stores connector-saved PDFs as `imported_url` with real
files) AND pdf/plain contentType OR `.pdf`/`.txt` filename.
Child notes: `note_html_to_text` (tags drop, `br`/block tags -> newlines with
runs collapsed to one, named + numeric entities decode) and `merge_child_notes`
(order by `data.dateAdded` ascending; each note -> `Title` line, `---`
separator, body; blocks joined by one blank line; all-empty -> `None`).
The merged text lands in `user_notes` via `user_notes_by_key` (the same
keyed-by-Zotero-item pattern as `tags_by_key`); `notes_merged_count` reports
it. Blank lines are reserved as the block separator - note text never
contains one.

### Key-based exclusion + version guard (`commands/zotero.rs`)

Deselection is keyed by Zotero item key, never positional indices; unknown
excluded keys are ignored (`skippedByUser` counts known keys only). The
preview fills the shared `ImportPreview.duplicate_count` via
`commands::import::count_library_duplicates` (canonical-DOI check of the
valid records against the current library, one short DB lock) - the same
early duplicate signal the RIS/BibTeX parse commands produce. The review
step's "Skip" checkbox (default on) flows into the command's
`skip_duplicates` arg: library-DOI duplicates are dropped in the db phase
(records and keys stay aligned) and counted as `skipped_duplicates`;
within-file duplicates and every other strategy still reach the classify
phase. The `Last-Modified-Version` captured at preview must match the
import fetch or the run aborts with nothing written. `articleKeys` align
1:1 with `previewArticles` (valid records only).

### Write client (`write_client.rs`)

Every write echoes `Zotero-Server-ID` (else 428). Authorization:
`POST /api/local/authorize {"appName":"Bango"}` blocks on a Zotero dialog
(120 s per-request timeout override); `200 {key, remember}`, deny `403
{"denied":true}`, >5 dialogs/min `429 + Retry-After`. `remember:false`
keys are single-use - a mid-run `401 Invalid or expired API key` maps to
the typed `KeyExpired`: the run aborts with "tick Remember" guidance and
the stored key is cleared (at most one authorize per export run, enforced
by the pure `decide_write_auth` helper). An authorize SEND timeout maps to
the distinct `DialogTimeout` (the user ignored the 120 s dialog - never
reported as "Zotero is not running"). New items POST in batches of 50
with a fresh 32-char `Zotero-Write-Token` per batch; the envelope's
`success`/`successful`/`unchanged`/`failed` maps drive counts
(`success_by_index` maps batch positions to created keys). Locally
generated item keys are NOT supported on new-item POSTs (live-verified
428 "Either If-Unmodified-Since-Version or 'version' property must be
provided for 'key'-based writes") - child notes always reference the
server-assigned parent key from the envelope. Files upload in
3 phases: `POST .../items/<key>/file` with md5/filename/filesize/mtime +
`If-None-Match: *` -> `{url, uploadKey}` or `{"exists":1}` -> bytes (`201`)
-> register `upload=<key>` (`204`). Deletes need
`If-Unmodified-Since-Version`. Attachment items carry a `title` and upload
`filename` from `export_mapping::build_attachment_title`: first author's
last name, a dash, the article title capped at 30 chars cut at a word
boundary, plus the file extension. The attachment item body is serialized
by `ordered_attachment_body` with `linkMode` before `filename`/
`contentType`: the local API applies fields in document order and rejects
a path field that precedes the link mode, and a no-key envelope error
carries the per-index `failed` reasons.

### Export mapping (`export_mapping.rs`)

Dates: `build_export_date(date, publication_year)` always emits the most
specific ISO form (`YYYY-MM-DD`/`YYYY-MM`/`YYYY`) - month/day come from
`parse_partial_date` (tolerant: ISO, `NOV 25`, month-only, month ranges ->
first month, `MM/YYYY`, `YYYY/MM/DD`, `Mon YYYY`) combined with the
authoritative `publication_year`; raw strings are never sent because
Zotero re-parses them (`NOV 25` + 2025 displayed as "Nov 25" with no
year - the reported bug). Notes: `split_note_blocks` splits user notes
back into `Title`/`---`/body blocks (free-form text -> one block, first
line the title) and `build_note_item_json` emits the
`{"itemType":"note","parentItem","note","tags":[]}` child with
HTML-escaped, `<br/>`-joined lines. The export core POSTs note batches
after the item batches (failures non-fatal: audit + counts); `notes` ->
`extra` stays the Imported-Notes path, `user_notes` never lands in the
item JSON.

### Stored settings

`zotero_api_key` (AES-256-GCM encrypted like the LLM/OpenAlex keys),
`zotero_server_id`, `zotero_last_collection_key`, `zotero_last_collection_name`.
The DB mutex is never held across HTTP calls, file reads, or the
attachment file copy (the import attachment phase uses the split pipeline:
`extract_full_text_data` unlocked, `commit_full_text_to_db` in a short
lock); export reads articles + settings in one short lock scope.
Attachment accounting: URL-only 302 Locations (`NonFileScheme`) count as
`attachment_skipped_count` (no audit error); every non-candidate child of a
non-duplicate article counts as skipped even when a candidate attaches
(pdf + epub). `skipped_count` in the import result carries the RIS-style
validation-skipped accounting (unsupported + missing-field records).

## Verification

`cargo test --test zotero_mapping_test --test zotero_client_test --test
zotero_connection_test --test zotero_import_test --test
zotero_export_mapping_test --test zotero_export_test --test
zotero_write_client_test`. Binding inventory:
`docs/test-plans/zotero-tests.md` (enforced by `scripts/check-test-inventory.sh`).
Live read-API facts were verified against Zotero 10.0.1; the write contract
is reproducible via `scripts/zotero_write_probe.sh --write`, the
3-phase file upload via `--upload` (probe items kept for inspection, keys
recorded in a temp state file) followed by `--cleanup`, and the full
metadata/date/tags/notes export round-trip via `--meta` (26 live
assertions: every metadata field, tags, ISO date variants incl.
year-only, and child notes) followed by `--cleanup`.

## Child DOX Index

No child `AGENTS.md` files.
