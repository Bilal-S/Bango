# batch_import/

## Purpose

4-phase batch import processor. Scans the Bango Documents directory for files
produced by external tools and imports them into the article database by DOI
match.

## Ownership

- Owns: `mod.rs` (`BatchImportRunner`, `BatchImportProgress`,
  `BatchImportState` managed state, `start_batch_import` / `cancel_batch_import`
  / `get_batch_import_progress` Tauri commands), `full_text_phase.rs`,
  `citations_phase.rs`, `translations_phase.rs`, `summary_phase.rs`,
  `embeddings_phase.rs`.
- Consumed by: frontend `settings-reprocessing.vue` (button "Import full text
  files").
- Reuses: `commands::full_text::attach_full_text_split`,
  `commands::references::import_references_inner`,
  `commands::summary::generate_article_ai_summary_inner`,
  `db::article_repo::get_articles_with_doi_info`.

## Local Contracts

Files keyed on `clean_doi_filename(normalized_doi)`, consistent with Citation
Chaser RIS naming (`{clean_doi}_references.ris`).

### Phase 1 - `full_text_phase.rs`

Scan `fulltext/` for `{cleaned_doi}.pdf` / `.txt`, attach via the split
pipeline `commands::full_text::attach_full_text_split` so the CPU-bound PDF
parse runs on `spawn_blocking` with no DB lock held; skips articles with
`has_full_text=true`; returns newly-attached IDs for Phase 3. Text-extraction
failures are handled inside `attach_full_text_inner` as soft-fallback attaches
with empty `full_text` + a `log_error` audit row, and hard attach failures
(missing file, copy error, DB write error) write a `log_error` audit entry in
addition to the in-memory progress errors.
Filename matching is case-insensitive: match-map keys are built from
`clean_doi_filename(lowercase doi)` and file stems are looked up lowercased,
so legacy mixed-case filenames still resolve.

### Phase 2 - `citations_phase.rs`

Scan `ris/` for `{cleaned_doi}_references.ris`, `_citations.ris`, `.ris`,
`.bib`; skips articles with `has_reference_details`/`has_citation_details`;
auto-detects RIS vs BibTeX by extension via extracted
`commands::references::import_references_inner`.
Discovery builds a one-pass lowercase filename index of `ris/`
(`build_lowercase_dir_index`) and resolves every probe against it (O(1) per
probe); when two files differ only in letter case, the
exactly-lowercase-named file wins the slot (deterministic on Linux).

### Phase 3 - `translations_phase.rs`

Enqueue `FullText` translation jobs for non-English newly-attached articles via
`enqueue_article_translation_inner` + poll until each completes; runs only when
`auto_translate=true`. **Pre-flight LLM-configured guard**
(`check_llm_configured_or_skip`, pure `&Connection` helper) short-circuits the
phase with the canonical `"Skipped: LLM not configured"` message + a
system-level audit record via `audit_repo::log_error` so the skip surfaces in
Diagnostics / Notification History instead of churning every article through
the worker's per-article failure path - mirrors the Phase 4 pre-flight pattern.

### Phase 4 - `summary_phase.rs` (parallel)

Generate AI summaries for newly-attached articles without an existing summary;
reuses extracted `commands::summary::generate_article_ai_summary_inner` so
behavior is identical to the article detail "Generate AI Summary" button
including the `include_section_summaries` flag. **Pre-flight LLM-configured
guard** (`llm_configured_with_audit`) short-circuits the phase with the same
`"Skipped: LLM not configured"` message + system-level audit record. The
sequential `for` loop was replaced by a `tokio::task::JoinSet` so all article
summaries are dispatched concurrently; the orchestrator's
`max_concurrent_requests` semaphore bounds real LLM concurrency (same pattern
as `wiki::ingest::batching::run_chunked_ingest`). Cancellation aborts remaining
tasks via `abort_all`.

### Phase 5 - `embeddings_phase.rs`

Generate embeddings for newly-attached articles. Snapshots `cancel_handle`
into an `Arc<AtomicBool>` ONCE before calling the embedding runner. See
`embedding/AGENTS.md` for the runner contract.

### Three coupled improvements (see `.worktrees/import_plan.md`)

#### (1) Parallel Phase 4

See Phase 4 above - the sequential loop was replaced by `JoinSet`.

#### (2) DOI-aware attach filename (`commands::full_text::attach_full_text_inner`)

Now takes `article_doi: Option<&str>`: when the article has a DOI, the
destination filename is `{clean_doi}.{ext}` (matches the on-disk batch-import
convention, no UUID suffix); no-DOI articles keep the
`{stem}_{article_id}.{ext}` fallback. A same-file short-circuit
(`place_file_in_storage`) skips the copy when source == destination (common in
batch import where the file is already in `fulltext/` with the correct name),
preferring a zero-copy `hard_link` with a byte-copy fallback. Applies to new
attaches only (no retroactive rename). Pure helper `compute_dest_filename` + 9
inline tests in `full_text.rs`.

#### (3) Short DB lock bursts in Phases 1 + 2 (Concern 3 root cause)

`run_full_text_phase` and `run_citations_phase` are now `async` and take
`&Mutex<Connection>`; they lock briefly for the initial discovery (resolve
storage dir + build the DOI match map via the pure `discover(conn)` helper),
release, then take one short lock burst per article for the DB write. Phase 1
uses the split `commands::full_text::attach_full_text_split` pipeline so the
CPU-bound PDF parse + text extraction runs on `spawn_blocking` with NO DB lock
held; only the DB-write portion (`commit_full_text_to_db`: update row + chunk
insert + audit entries + staleness flags) runs under the short burst. The pure
extract helper `extract_full_text_data` + the DB-write helper
`commit_full_text_to_db` are also reusable directly by callers that already
hold a `&Connection` (manual `attach_full_text` command, OpenAlex import). The
previous shape locked the connection ONCE at the top of a `spawn_blocking` and
held the guard across every per-article PDF parse, freezing every other
DB-touching IPC command for the whole phase. `tokio::task::yield_now()`
between articles lets the runtime flush progress events + give other commands
a turn.

`db::article_repo::get_articles_with_doi_info` loads all articles with a
non-null DOI + the `has_full_text` / `has_reference_details` /
`has_citation_details` / `has_ai_summary` flags in a single query to build the
DOI match map.

## Work Guidance

- Spawned `tokio::task` so the UI stays responsive and the user can navigate
  away; cancel token checked between items; emits `batch-import:progress`
  events.
- Frontend `settings-reprocessing.vue`: live progress bar with phase label +
  per-phase completed/total + overall percent + cancel button; per-phase
  summary lines surface skip messages - e.g. "Skipped: LLM not configured" -
  with a warning style via `phaseSkipMessage(phase)` so the user understands
  why a phase did nothing; listens to `batch-import:progress` events so it
  survives navigation.
- Terminal snapshot: after all five phases finish, `start_batch_import`
  emits one final `emit_progress` with the `BatchImportPhase::Complete`
  variant (phase label "Batch Import", `overall_percent = 100`,
  `is_running = false`) so the user sees an unambiguous "Batch Import / 100%"
  end state instead of the just-finished "Embeddings" label. `Complete` (6)
  is a terminal indicator, not a work phase.

## Verification

10 inline tests in `full_text_phase.rs` (DOI match map normalization +
collision + empty skip + secondary `article_id → DOI` index for the O(n)
per-article lookup) + `citations_phase.rs` (skip-when-has-details,
find-references, find-citations-independently, generic-ris-fallback,
generic-bib-fallback). End-to-end integration tests live in
`tests/batch_import_test.rs` (13 tests: Phase 1 attach + skip-already-attached
+ no-matching-DOI + no-DOI-article; Phase 2 refs + citations + independent +
skip-already-has-details; full-pipeline idempotency; multiple articles with
mixed files; Phase 3 pre-flight skip + audit + proceed) +
`tests/full_text_split_test.rs` (12 tests: isolated coverage of the split
pipeline `extract_full_text_data` + `commit_full_text_to_db` +
`attach_full_text_split` - figures-flag true/false, soft-fallback on invalid
PDF, DOI-aware destination filename, chunk write, extraction-failure audit,
end-to-end composition; the monolithic `attach_full_text_inner` path is covered
by `tests/figures_flag_test.rs`). Phase 3 (live translation) + Phase 4 (AI
summaries) require a live LLM and are not covered end-to-end; the pre-flight
LLM gate is unit-tested via the pure `check_llm_configured_or_skip` helper, and
the `generate_article_ai_summary_inner` core is tested via the existing
`summary_engine_test.rs` mock-LLM path.

## Child DOX Index

No child `AGENTS.md` files. This module owns five phase files + `mod.rs` with
no further durable boundaries.