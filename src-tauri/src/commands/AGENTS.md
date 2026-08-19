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
`commands/articles.rs` already set them. Tested in `tests/tags_labels_test.rs`.

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
counts surface in the success toast. Tested in `tests/merge_tags_labels_test.rs`
(15 tests incl. `merge_tag_no_dangling_overlap_rows` CASCADE regression +
`merge_tag_chain_safe` chained-merge safety).

The shared `audit_repo::write_tag_label_audit` helper is the canonical loop for
multi-article tag/label audit entries, reused by both the bulk commands (via
`write_bulk_tag_label_audit` in `commands/articles.rs`) and the merge commands.

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
integration tests: `tests/tags_labels_test.rs`,
`tests/merge_tags_labels_test.rs`, `tests/legacy_upgrade_test.rs`. Binding
inventories: `docs/test-plans/criteria-generation-tests.md` and
`docs/test-plans/search-strategy-tests.md` (enforced by
`scripts/check-test-inventory.sh` via `npm run check:all`).

## Child DOX Index

No child `AGENTS.md` files.
