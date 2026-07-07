# translation/

## Purpose

Plan-A permanent-rewrite translation of non-English articles to English.
Rewrites the working `articles` (title/abstract/full_text) and `article_chunks`
rows with translated English content so downstream screening and summary
workflows consume English text without pass-through prompts. Originals are
preserved in `article_original_content` and `article_original_chunks`.

## Ownership

- Owned by the language-plan-v2 milestone (`.worktrees/language-plan-v2.md`) +
  the translation-3 batched-chunk optimization
  (`.worktrees/translation-3-plan.md`).
- Bound to the binding test inventories in
  `docs/test-plans/language-plan-v2-tests.md` (21 Rust + 5 TS rows) and
  `docs/test-plans/translation-3-tests.md` (16 Rust rows: 12 inline `engine.rs`
  unit tests + 4 integration tests).

## Local Contracts

- There is **no `translation_jobs` table**. The DB-backed progress record lives
  on the `articles` row: `translation_status` (`none`|`queued`|`running`|
  `succeeded`|`failed`) + `is_translated` (0/1) + `translation_error` +
  `translated_at`.
- The in-memory queue is a `tokio::mpsc` channel owned by a single worker task
  spawned once at app startup in `lib.rs`. Crash recovery
  (`reenqueue_stranded_on_startup`) finds any article with
  `translation_status IN ('queued','running') AND is_translated = 0`. Because
  `STARTUP_STRANDED_CAP = 0` (decision: no auto-recovery on restart), **no**
  stranded job is re-enqueued — every stranded row is marked `failed` with a
  `translation_error` audit note. The user selectively retranslates via the
  manual translate button on the article detail header (the enqueue gate
  accepts `failed`). Raising the cap to a positive `N` re-enables bounded
  re-enqueueing of the first `N` stranded jobs, choosing `FullText` when
  `has_full_text = 1` else `MetadataOnly`.
- The enqueue path reads `translation_status` before sending to the channel:
  - `none` or `failed` -> write `queued` then send.
  - `queued`, `running`, or `succeeded` -> skip silently.
  - `is_translated = 1` -> skip silently regardless of status.
- Write-back is a single `rusqlite::Transaction`: delete+insert translated
  `article_chunks`, update `articles`, write `translation`/`translation_error`
  audit entry, commit. On any error the transaction rolls back; no partial
  rows reach `articles` or `article_chunks`.
- `articles.language` is the sole original-language source and is immutable
  after translation. `is_translated = 1` with `language = 'French'` means
  "originally French, now translated to English; originals in
  `article_original_content`".
- Translation skip-policy gate: `language::should_skip_translation(language)`
  returns `true` for English OR absent/blank language (plan §F.2 + §G). All
  enqueue/engine call sites use this gate - never `is_english_language` alone,
  which answers only the narrow question "is this an English-language marker?"
  (absent/blank is NOT English, but it IS skip-translation because the language
  is genuinely unknown and translating unknown-language text wastes LLM tokens).
- Abstract translation qualifier: hybrid ASCII-range + top-21 English-stopword
  heuristic (see `language.rs`, includes `or`). No external language-detection
  crate.
- The translation engine owns a thin `TranslationLlmClient` that implements the
  existing `screening::llm_client::LlmClient` trait. Before each delegated
  orchestrator call it logs `job_id` (article UUID) and `part_id` (chunk index).
  It delegates via `send_with_type(LlmRequestType::Translation)`. The
  `LlmClient` trait and `send_with_type` default method are not widened.

## Work Guidance

- All LLM calls MUST go through `LlmOrchestrator` (per CLAUDE.md). The
  `TranslationLlmClient` wraps it; never call `llm::client` directly.
- Translation runs as a background job (manual click, import trigger,
  full-text attach trigger, batch-import Phase 3, or the screening pre-step).
  Never block the IPC handler on a translation.
- Auto-translate only runs when `app_settings.auto_translate` is `true`
  (default DISABLED / opt-in; absent/garbage falls back to disabled). Decision
  (a): the default was flipped from `true` to `false` so imports do not
  silently trigger background translation + LLM cost. The user must enable it
  explicitly in Settings.
- Import + full-text attach triggers call the lock-free
  `try_enqueue_translations_for_import(app, &Mutex<Connection>, ids)` helper
  (Tier 1a/1b). Callers MUST drop their own `MutexGuard` before calling it so
  the import lock is not held across the enqueue round-trip; the helper
  re-locks briefly for one filtered read (`get_translatable_import_ids`) + one
  bulk write (`mark_translation_queued_batch`).
- Screening pre-step (decision b): when `auto_translate` is enabled,
  `commands::screening::run_pre_screening_translation` enqueues `MetadataOnly`
  jobs for unscreened working non-English articles and awaits them via the
  `TranslationDoneBus` BEFORE the screening engine runs, so the screening LLM
  reads English text. Emits `screening:translation-progress` events with a
  per-article counter so the UI shows "Translating N/M articles...". Skipped
  entirely when `auto_translate` is off.
- `TranslationDoneBus` (`translation/wait.rs`) is a `tokio::sync::broadcast`
  managed-state channel the worker emits on after each job finishes (success or
  failure). `wait_for_article_translation` subscribes + falls back to a 60s
  sanity poll so a missed event never deadlocks the caller. Batch-import Phase
  3 and the screening pre-step both consume it.
- **Frontend badge refresh** (the Rust worker emits `translation:complete`
  only on completion, so the `use-translation.ts` composable exposes an
  `onTranslationQueued` callback fired immediately after a successful
  `enqueue_article_translation` invoke). `article-detail-panel.vue` wires both
  `onTranslationQueued` and `onTranslationComplete` to `emit('refreshArticle')`
  so the status badge flips to the "Translation Queued" spinner chip right
  away - without the immediate refresh the badge stays stale through the whole
  `queued`→`running` window (which can take minutes for full-text jobs).
- Translation must complete before screening and summary generation consume
  the article. Batch import Phase 4 (summaries) gates per article on
  `translation_status` leaving `running`.
- **Batched chunk dispatch** (`engine.rs`): `translate_full_text`
  packs chunks into context-window-sized batches (`build_chunk_batches`,
  mirroring `wiki/ingest/batching.rs::build_ingest_prompt_batches` with
  `INPUT_BUDGET_FRACTION = 0.4`, floor `MIN_BATCH_INPUT_CHARS = 4_000`, cap
  `MAX_BATCH_INPUT_CHARS = 80_000`) and sends each batch as ONE LLM call
  carrying a JSON-lines payload (`{"<chunk_id>": "<text>"}` per line). The
  model responds with a single JSON object mapping each chunk_id to its
  English translation; `parse_batch_translation_response` (tolerant of
  markdown fences + a regex `{...}` fallback) fills a pre-sized slot per
  chunk_id. This reduces a 46-chunk article from 46 per-chunk calls to
  ~2-3 batched calls.
- **Resend loop with cap** (`engine.rs`): any chunk the model skipped or
  returned empty is collected into a resend round, which repacks ONLY the
  missing chunk_ids via `build_chunk_batches_for_indices` (preserving the
  ORIGINAL chunk ids in both the prompt keys and the returned batch indices
  so slots fill without remapping). After `MAX_RESEND_ITERATIONS = 2`
  rounds, any still-missing chunk FAILS the job with a clear audit detail
  naming the missing chunk ids. This is stricter than the previous
  skip-empty lenience (an unfilled slot now fails rather than silently
  dropping a chunk from the stitched `full_text`) - no silent gaps reach
  the translated text.
- **Concurrency**: batches are dispatched concurrently via
  `futures::future::join_all`; real parallelism is bounded by the
  `LlmOrchestrator`'s semaphore (`max_concurrent_requests` from
  `LlmConfig`), so a single-request config serializes while a
  high-concurrency config parallelizes automatically. The resend round is a
  follow-up `join_all` after the prior round completes. The metadata LLM
  call (title + abstract) still runs sequentially before chunk dispatch.
- **`context_window_tokens` plumbing**: the worker extracts
  `config.context_window_tokens` from the concrete `LlmConfig` BEFORE
  constructing the `TranslationLlmClient` and passes it as a fourth
  parameter to `translate_full_text`. It cannot be read inside the engine
  from the `&dyn LlmClient` trait object (the trait has no config
  accessor; widening it would pollute `screening::llm_client`).
- **Error handling**: an LLM error on any batch fails the whole job
  (mirrors the previous fail-on-first-error semantics); other in-flight
  batches in the same round complete harmlessly (bounded by the
  orchestrator). A parse failure on a batch response records every
  expected id in that batch as missing, triggering a full-batch resend
  (bounded by the cap above). Tested in `auto_translate_full_text_test.rs`
  (`full_text_translation_produces_english_chunks_and_full_text`,
  `parallel_chunk_dispatch_preserves_input_order`,
  `batched_translation_resends_missing_chunks`,
  `batched_translation_fails_after_resend_cap`) + 12 inline unit tests in
  `engine.rs` covering the pure packer + parser helpers. Binding inventory:
  `docs/test-plans/translation-3-tests.md`.
- `#[must_use]` on pure helpers (`language.rs` detection functions).
- `parse_metadata_translation` is strict: an empty title OR an empty abstract
  section is a parse failure (returns `None`), so a malformed LLM response
  cannot overwrite the working title/abstract with an empty string.
- Manual translate commands (`enqueue_article_translation`,
  `retry_translation_job`) choose the job kind from `has_full_text` via the
  shared `choose_job_kind` helper, so a manual click on a full-text article
  translates the full text + chunks, not just the metadata.

## Verification

- `cargo test` (integration tests in `src-tauri/tests/`):
  `translation_queue_test.rs`, `language_detection_test.rs`,
  `multilingual_sections_test.rs`, `multilingual_assets_test.rs`,
  `screening_translation_integration_test.rs`,
  `summary_translation_integration_test.rs`, `batch_import_translation_test.rs`,
  `v001_v003_schema_parity_test.rs`.
- `npm run check:all` includes `check:test-inventory`, which enforces the
  binding inventories in `docs/test-plans/language-plan-v2-tests.md` AND
  `docs/test-plans/translation-3-tests.md` (the batched-chunk-dispatch
  inventory: 12 inline `engine.rs` unit tests + 4 integration tests in
  `auto_translate_full_text_test.rs`).

## Child DOX Index

No child `AGENTS.md` files. This module owns five files (`mod.rs`,
`engine.rs`, `language.rs`, `wait.rs`, `worker.rs`) with no further durable
boundaries.
