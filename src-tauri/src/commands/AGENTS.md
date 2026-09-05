# src-tauri/src/commands/

## Purpose

Tauri command handlers, one file per feature area: the thin IPC layer over
the repos and engines.

## Ownership

- All handlers are registered in the single `invoke_handler!` list in
  `lib.rs::run()` (see `src-tauri/src/AGENTS.md`).
- The article state machine + hard-delete cascade contracts stay in
  `src-tauri/src/AGENTS.md` (they span `commands/articles.rs` + the repos).

## Local Contracts

### `startup.rs`

Exposes `get_startup_status` and `perform_legacy_upgrade` (one-shot:
`export_legacy_project` -> write backup to `app_data_dir` -> `rebuild_schema`
-> journal reload -> `import_project`; backup file is never deleted).
**Loop-safety**: a webview `window.location.reload()` runs in the same Rust
process, so managed state is not recomputed. To prevent an endless reload loop
after a successful upgrade, `get_startup_status` re-probes the LIVE schema on
every call (falling back to the setup-time snapshot only if the live probe
errors), and `perform_legacy_upgrade` updates the managed `StartupStatus`
snapshot (now a `Mutex<SchemaStatus>`) post-success. Pure decision logic lives
in `legacy_upgrade_needed(live, fallback)`; the frontend adds a third
sessionStorage-based guard in `use-startup-upgrade.ts`.

### `tags.rs` + `labels.rs`

Tag & Label management commands (v6.9 standard-taxonomy surfacing). `tags.rs`
owns `STANDARD_STUDY_TAGS` (20 methodology/study-type tags) injected into the
`suggest_tags` prompt as a `## Standard Study-Type Tags` section instructing
the LLM to include up to 4 when relevant. `labels.rs` owns
`STANDARD_WORKFLOW_LABELS` (12 workflow-state labels) injected into the
`suggest_labels` prompt similarly. All standard entries are pre-validated to
pass the 35-char `sanitize_tag_or_label_name` gate (see `screening/AGENTS.md`)
so the backend sanitizer never silently truncates them.

**Staleness-flag contract (bugfix)**: `delete_tag` and `delete_label` set both
`mark_biblio_needs_refresh` + `mark_wiki_needs_refresh` after the repo delete.
These were previously the only two tag/label mutation paths that omitted the
flags, silently desyncing the keyword co-occurrence network
(`biblio_repo/networks/keywords.rs`) and the wiki concept hubs
(`wiki/ingest/concepts.rs`) after a delete - every other tag/label mutation in
`commands/articles.rs` already set them. Tested in `tests/db/tags_labels_test.rs`.

**Merge ("Replace with...") contract**: `merge_tag` / `merge_label` commands
delegate to `pub fn merge_tag_inner` / `merge_label_inner` (testable without
`State<DbState>`). Each runs inside one `unchecked_transaction`: compute
overlap count BEFORE the destructive `UPDATE OR IGNORE`, call
`tag_repo::merge_tags` / `label_repo::merge_labels` (CASCADE removes overlap
junction rows), write one coalesced `tag_remove` / `label_remove` audit entry
per *reassigned* article via the shared `audit_repo::write_tag_label_audit`
helper (single-entry bulk pattern; detail string
`Replaced "A" -> "B" (merge)` carries both halves), bump `changed_at`, set both
staleness flags. The `MergeResult` (`reassigned_count` excludes co-occurrence
overlaps; `already_had_survivor_count` reports them separately). The
pre-confirm dialog shows an honest upper bound (`from.articleCount`); the real
counts surface in the success toast. Tested in `tests/db/merge_tags_labels_test.rs`
(15 tests incl. `merge_tag_no_dangling_overlap_rows` CASCADE regression +
`merge_tag_chain_safe` chained-merge safety).

The shared `audit_repo::write_tag_label_audit` helper is the canonical loop for
multi-article tag/label audit entries, reused by both the bulk commands (via
`write_bulk_tag_label_audit` in `commands/articles.rs`) and the merge commands.

### `full_text.rs` - async chunk-rebuild pipeline (Settings "Rebuild text chunks")

Replaces the old sync `rebuild_article_chunks` command, which froze the UI
(DB mutex held across every PDF parse) and crashed the app
(`tokio::task::spawn` from the sync IPC handler on the main/GTK thread, where
no Tokio reactor exists - panic + process abort after every successful
rebuild). New contract:

- Commands: `start_rebuild_chunks` (async; ATOMIC run-slot claim via
  `claim_run_slot` - check + cancel reset + snapshot reset in one critical
  section, so overlapping starts can never double-spawn; a discovery error
  releases the slot via `release_run_slot`), `cancel_rebuild_chunks`
  (`Arc<AtomicBool>` token), `get_rebuild_chunks_progress`
  (restore-on-mount). State: `RebuildChunksState` managed in `lib.rs`. Events:
  `chunk-rebuild:progress` (`RebuildChunksProgress`, camelCase).
- Split-pipeline lock discipline (mirrors batch import Phase 1): CPU-bound
  `extract_sections` + `chunk_sections` on `spawn_blocking` with NO DB lock;
  one short lock burst per article wrapping `replace_chunks_for_article` +
  `delete_embeddings_for_article` in a single `unchecked_transaction`
  (a failed embedding DELETE rolls the chunk REPLACE back - chunks and
  vectors can never diverge on a mid-write failure); `yield_now()` between
  articles.
- All three per-article failure modes (missing `full_text_file_name`, missing
  on-disk file, extraction/write error) increment `failed`, push a message
  embedding the article id into `RebuildChunksProgress.errors`, and write a
  `log_error` audit row (the first two were previously silent). `errors` is
  capped at `MAX_PROGRESS_ERRORS` (50) so the per-event snapshot clone stays
  bounded; `failed` always reports the true total and
  `finalize_rebuild_progress` appends a "... and N more failures (see
  Diagnostics)" tail. `finalize_rebuild_progress` also ORs `is_cancelled`
  from the token (covering a cancel that lands during the embedding cascade)
  and never recomputes `skipped` (it stays == translated skips).
- **Translated guard**: candidates with `is_translated = 1` are never
  re-chunked (working chunks are the English translation; the on-disk PDF is
  the original language). They count as `skipped_translated` and feed the
  backfill-only embedding scope. See `translation/AGENTS.md`.
- **Embedding cascade** (inside the spawned task, after the loop): two
  `generate_embeddings_inner` calls with `force = false` - (A) regenerate the
  re-chunked ids (their embedding rows were deleted by the loop, so missing
  rows drive full regeneration and orphans cannot survive), (B) backfill-only
  for translated ids (existing fresh English rows are never re-embedded).
  Gating (LLM configured, provider supports embeddings) is owned by the
  runner/director; skips surface as `embeddingSummary` lines
  ("Embeddings skipped: LLM not configured" / "... provider does not support
  embeddings"), not errors. Skip reasons are matched via
  `SkipReason::as_str()` (the canonical serialized form in
  `embedding/director.rs`) - never against `{:?}` Debug output. The shared
  cancel token also aborts the cascade.
- Frontend: `settings-reprocessing.vue` renders the live progress bar
  (phase label, phase-mapped percent - chunking owns the 0-90 band, the
  cascade owns 90-100 driven by `embedding:progress` counters - counts +
  translated-skip summary, per-article error list, cancel button) and
  restores it on mount.
- Unchanged: the screening-path helpers `ensure_chunks_inner`,
  `ensure_chunks_for_full_text_articles(_with_progress)` stay byte-identical
  (they run inside the screening task's own lock scope).


### Criteria harmonization (inclusion/exclusion division of labor)

Inclusion criteria define the SCOPE of a review; exclusion criteria define
INDEPENDENT removal reasons that would otherwise pass the inclusion filter
(publication type, language, animal/in-vitro-only studies, duplicates). An
exclusion criterion must NEVER merely negate an inclusion criterion: the
screening engine already excludes any article that matches no inclusion, so a
negating exclusion is doubly redundant AND bloats search-strategy queries with
self-canceling NOT clauses that fail to run.

Enforced across three prompt builders (each a `pub fn` pure helper with binding
tests):
- `commands::criteria::build_criteria_generation_prompt` - surfaces existing
  opposite-type criteria, caps exclusions lower (6 vs 8), and carries the
  "do not negate" guidance.
- `commands::criteria::build_check_rules_prompt` - the holistic "review my
  ruleset" review flags negations in ALREADY-EXISTING criteria and recommends
  deleting them (catches the generation guard's blind spot).
- `commands::search_strategy::build_search_strategy_prompt` - tells the LLM to
  drop negating exclusions rather than encoding them as self-canceling NOT
  clauses.
Binding inventory: `docs/test-plans/criteria-generation-tests.md` and
`docs/test-plans/search-strategy-tests.md` (enforced by
`scripts/check-test-inventory.sh` via `npm run check:all`).

## Work Guidance

- All LLM calls flow through `LlmOrchestrator` (`llm/AGENTS.md`), never
  `client::send_chat_completion` directly.
- All `DbState.conn` locks route through `db::lock_conn` (`db/AGENTS.md`).

## Verification

See `src-tauri/src/AGENTS.md`: `npm run check:all` + `cargo test`. Relevant
integration tests: `tests/commands/chunk_rebuild_test.rs` (async chunk-rebuild loop,
translated guard, embedding-cascade helpers), `tests/db/tags_labels_test.rs`,
`tests/db/merge_tags_labels_test.rs`, `tests/export/legacy_upgrade_test.rs`. Binding
inventories: `docs/test-plans/criteria-generation-tests.md` and
`docs/test-plans/search-strategy-tests.md` (enforced by
`scripts/check-test-inventory.sh` via `npm run check:all`).

## Child DOX Index

No child `AGENTS.md` files.
