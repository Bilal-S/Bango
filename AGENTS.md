# DOX framework

- DOX is highly performant AGENTS.md hierarchy installed here
- Agent must follow DOX instructions across any edits

## Core Contract

- AGENTS.md files are binding work contracts for their subtrees
- Work products, source materials, instructions, records, assets, and durable docs must stay understandable from the nearest applicable AGENTS.md plus every parent AGENTS.md above it

## Read Before Editing

1. Read the root AGENTS.md
2. Identify every file or folder you expect to touch
3. Walk from the repository root to each target path
4. Read every AGENTS.md found along each route
5. If a parent AGENTS.md lists a child AGENTS.md whose scope contains the path, read that child and continue from there
6. Use the nearest AGENTS.md as the local contract and parent docs for repo-wide rules
7. If docs conflict, the closer doc controls local work details, but no child doc may weaken DOX

Do not rely on memory. Re-read the applicable DOX chain in the current session before editing.

## Update After Editing

Every meaningful change requires a DOX pass before the task is done.

Update the closest owning AGENTS.md when a change affects:

- purpose, scope, ownership, or responsibilities
- durable structure, contracts, workflows, or operating rules
- required inputs, outputs, permissions, constraints, side effects, or artifacts
- user preferences about behavior, communication, process, organization, or quality
- AGENTS.md creation, deletion, move, rename, or index contents

Update parent docs when parent-level structure, ownership, workflow, or child index changes. Update child docs when parent changes alter local rules. Remove stale or contradictory text immediately. Small edits that do not change behavior or contracts may leave docs unchanged, but the DOX pass still must happen.

## Hierarchy

- Root AGENTS.md is the DOX rail: project-wide instructions, global preferences, durable workflow rules, and the top-level Child DOX Index
- Child AGENTS.md files own domain-specific instructions and their own Child DOX Index
- Each parent explains what its direct children cover and what stays owned by the parent
- The closer a doc is to the work, the more specific and practical it must be

## Child Doc Shape

- Create a child AGENTS.md when a folder becomes a durable boundary with its own purpose, rules, responsibilities, workflow, materials, or quality standards
- Work Guidance must reflect the current standards of the project or user instructions; if there are no specific standards or instructions yet, leave it empty
- Verification must reflect an existing check; if no verification framework exists yet, leave it empty and update it when one exists

Default section order:
- Purpose
- Ownership
- Local Contracts
- Work Guidance
- Verification
- Child DOX Index

## Style

- Keep docs concise, current, and operational
- Document stable contracts, not diary entries
- Put broad rules in parent docs and concrete details in child docs
- Prefer direct bullets with explicit names
- Do not duplicate rules across many files unless each scope needs a local version
- Delete stale notes instead of explaining history
- Trim obvious statements, repeated rules, misplaced detail, and warnings for risks that no longer exist

## Closeout

1. Re-check changed paths against the DOX chain
2. Update nearest owning docs and any affected parents or children
3. Refresh every affected Child DOX Index
4. Remove stale or contradictory text
5. Run existing verification when relevant
6. Report any docs intentionally left unchanged and why

## User Preferences

When the user requests a durable behavior change, record it here or in the relevant child AGENTS.md

## Child DOX Index

Top-level source directories. Child `AGENTS.md` files exist under
`src-tauri/src/llm/` and `src-tauri/src/translation/`; the entries below
describe every durable boundary so agents can locate the right area. Create a
child `AGENTS.md` under a folder only when that folder grows its own local rules.

- **`src-tauri/src/`** - Rust backend (Tauri 2.x). Article state machine (§4.2 of the spec): **moving an article back to `working` from any other status (`included`/`rejected`/`duplicate`) always resets the screening flags (`screened_at = NULL`, `screening_error = 0`)** so the article becomes eligible for re-screening on the next run. Both `update_article_status` and `bulk_update_article_status` enforce this rule. Without the reset the stale `screened_at` timestamp survives the status change and excludes the article from `get_next_unscreened_working_batch`, leaving it stuck in a "previously screened" limbo that surfaces in the Error tab even though `screening_error` is `0`. The audit entry notes "(screening flags reset for re-screening)" when the reset fires. Tested in `tests/status_transition_screening_flags_test.rs`. Owned modules: `db/` (repos +
  `migrations/`), `models/`, `commands/`, `llm/` (orchestrator pattern; has its
  own `AGENTS.md` covering the retry + shared-client + payload-normalization
  contract for the Windows intermittent "insufficient permissions" fix),
  `screening/`,
  `dedup/`, `ris/`, `bibtex/`, `prisma/`, `export/`, `scraping/`, `crypto/`, `wiki/`
  (LLM knowledge base; see `wiki/` entry below), `utils/` (pure helpers:
  `batch_import/` (4-phase batch import processor; see `batch_import/` entry
  below),
  `pdf_extract.rs` (incl. **legacy-CJK mojibake recovery** via `encoding_rs` +
  `chardetng`: when `unpdf` returns raw Shift-JIS/EUC-JP/CP949/GB18030 bytes
  as Latin-1 code points - the common failure mode for CJK PDFs whose fonts
  lack a ToUnicode CMap - the `recover_mojibake` pass detects the C1
  control-char signature and re-decodes the bytes to correct Unicode before
  header/footer stripping; tested in `tests/pdf_mojibake_test.rs`),
  `sections.rs`, `chunking.rs`, `text_tokens.rs` [Tier 3 shared
  tokenizer for FTS5 BM25 + screening chunk scoring]). App entry
  is `lib.rs` (`run()`), which registers all `#[tauri::command]` handlers in one
  `invoke_handler!` list, auto-loads the bundled `journal_index.db` on first
  startup, and shows a native modal dialog (via `tauri-plugin-dialog`) if
  `run_migrations` fails in `.setup()` - the message names the resolved
  `app_data_dir` path and the three database files (`bango.db`,
  `bango.db-wal`, `bango.db-shm`) to back up or delete before restarting.
  Platform DB paths (`BonCode.Bango` identifier): Windows
  `%APPDATA%\BonCode.Bango\bango.db`, macOS
  `~/Library/Application Support/BonCode.Bango/bango.db`, Linux
  `~/.local/share/BonCode.Bango/bango.db`.
  - **`src-tauri/src/db/chunk_repo.rs`** - Tier 3 article chunk storage (`article_chunks`
    table, created by migration v003; was in v002 pre-release but v002 was reverted to
    `wiki_index_manifest`-only after deployment, so the FTS5 drop + article_chunks +
    audit_entries rebuild moved to v003). Populated at attach time by
    `commands::full_text::populate_chunks_for_attached_text` (extract_sections +
    chunk_sections) and cleared on detach. Consumed by `screening::chunk_retrieval` for
    enhanced/two_stage screening evidence. Exposes `replace_chunks_for_article`,
    `list_chunks_for_article`, `delete_chunks_for_article`, `count_chunks_for_article`,
    `get_articles_with_full_text_missing_chunks` (screening-start guard,
    `has_full_text=1` AND non-empty `full_text` AND zero chunks; excludes
    soft-fallback empty-text attaches so the guard does not retry-pam invalid
    PDFs on every screening run), `get_articles_with_full_text` (Settings
    "Rebuild text chunks" button, `has_full_text=1` regardless of chunks, so a
    corrupted/partial/outdated set is repaired), `count_articles_with_full_text`.
    Tested in `tests/chunk_retrieval_test.rs`.
  - **`src-tauri/src/screening/chunk_retrieval.rs`** - Tier 3 criteria-targeted chunk
    retrieval (pure, `#[must_use]`). `rank_chunks_by_criteria(chunks, inc, exc, top_k,
    max_chunk_words, budget)` scores chunks by criteria-token TF density (shared
    `utils::text_tokens` tokenizer), boosts Methods-section matches, filters oversized
    chunks, and enforces a per-article word budget. Also owns the canonical
    `format_chunks_as_evidence(chunks) -> Option<String>` (the `## Supporting Evidence`
    body formatter); `engine::format_chunks_as_evidence` and `evidence::resolve_evidence`
    both delegate here so the chunks-only output stays byte-identical across modes.
    Constants: `DEFAULT_TOP_K=2`, `DEFAULT_MAX_CHUNK_WORDS=600`, `METHODS_BOOST=0.25`,
    `DEFAULT_CHUNK_BUDGET_PER_ARTICLE=2400`. 11 inline tests + the §T3.7 inventory.
  - **`src-tauri/src/screening/` Tier 3 Phases C/D/E (enhanced + two_stage modes)** -
    **Custom-logic governance contract** (v8.1): when `app_settings.screening_custom_logic`
    is present and non-empty (same trim + non-empty gate as the prompt's `## Custom Screening
    Instructions` emitter), the LLM's `decision` is final and the generic §4.1 priority
    resolver (`resolution::resolve_decision`) is NOT applied - the resolver cannot understand
    combinatorial AND/OR/hard-exclusion rules, so it must not second-guess the LLM. The engine
    computes `has_custom_logic` once per run and routes both stage-1 and stage-2 decisions
    through `resolution::finalize_decision(llm_decision, input, has_custom_logic)`, which
    returns the LLM decision verbatim when custom logic is in force and otherwise delegates
    to `resolve_decision` (drop-in `&str` return via lifetime). The `[App override: ... favored
    due to priority resolution]` annotation is naturally suppressed because
    `final_decision == llm_decision` whenever custom logic governs. Projects without custom
    rules get byte-identical behavior to the historical resolver. Tested in
    `tests/resolution_test.rs` (4 `finalize_decision` tests + the 7 original `resolve_decision`
    tests, signature unchanged).
    **Single-attempt LLM call per batch** (v8.1): the screening engine makes one
    `send_with_type` call per batch. The previous outer 429 retry loop (3 extra
    attempts, each bounded by the orchestrator's 600s cap, no cancel-token
    check) was removed because (a) the inner `client::send_with_retry` already
    handles transient 429/408/5xx with bounded retry (3 attempts, exponential
    backoff 1s/2s/4s capped at 10s, honors `Retry-After`), and (b) the outer
    loop multiplied the 600s cap by 4 (up to 40 min per batch) and ignored the
    cancel token, so Stop had no effect during the retry sleeps - the user
    observed screening stuck at "30 of 50" for >10 minutes. Any error from the
    orchestrator (including sustained 429) now marks the batch as errors and
    moves on; the next batch benefits from `request_delay_ms` + the
    orchestrator's concurrency semaphore. Sustained rate limits should be
    mitigated by raising `request_delay_ms` in LLM settings, not by an outer
    loop that ignores cancellation.
    **Per-request-type timeout** (v8.2): the orchestrator's per-call wall-clock
    cap is now per-`LlmRequestType` via the pure `#[must_use]` helper
    `llm::orchestrator::timeout_for(request_type) -> Duration`. Screening (both
    stage-1 `Screening` and stage-2 `EnhancedScreening`) uses
    `SCREENING_TIMEOUT_SECS = 120` (2 minutes); all other request types keep
    the 10-minute default `LLM_TIMEOUT_SECS = 600`. Combined with the single-
    attempt-per-batch contract above, a hung or slow screening call now surfaces
    as an error within ~2 minutes instead of stalling the run. Tested in
    `tests/llm_orchestrator_test.rs` (3 `timeout_for` tests).
    **Immediate Stop + transient-error handling** (v8.3): two coupled changes
    addressing the critique in `.worktrees/llmscreen.md`. (a) Both stage-1 and
    stage-2 LLM calls are wrapped in `tokio::select!` against a
    `tokio::sync::Notify` cancel signal. Clicking Stop drops the in-flight
    future (cancelling the underlying `reqwest` request) within milliseconds
    instead of waiting up to 2 minutes; the response is DROPPED (no DB write,
    no error marking). The `cancel()` method calls `notify_waiters()` in
    addition to setting the bool flag. (b) Transient LLM errors (429, 401/403
    Windows transient, 5xx, timeout, transport) now leave articles UNSCREENED
    (no `screening_error`, no `screened_at`) so the next run picks them up
    naturally - no manual "Reset Errors" workaround. A new
    `after_sequence_id: Option<i64>` cursor on
    `get_next_unscreened_working_batch` + per-run `last_attempted_seq` tracking
    ensures the current run advances past failed batches instead of re-fetching
    infinitely. The pure `#[must_use]` helper
    `screening::engine::is_transient_llm_error(e) -> bool` classifies errors by
    inspecting the message string (all LLM errors are `AppError::Import(String)`).
    Non-transient errors (malformed JSON, parse mismatch) keep the existing
    batch-error-marking behavior. The Windows 401/403 `insufficient permissions`
    retry rationale is documented in the `is_retryable_response` doc-comment in
    `client.rs` and in `src-tauri/src/llm/AGENTS.md`. Tested in
    `tests/screening_engine_test.rs` (8 `is_transient_llm_error` tests) +
    `tests/article_query_test.rs` (5 `batch_fetch_offset_*` tests).
    **Auto-stop + fixed phantom progress + actionable timeout** (v8.4): three
    improvements addressing `.worktrees/llmscreen2.md`. (a) Auth failures
    (401/403 without the Windows transient body) stop the run immediately
    (threshold = 1) via new pure `#[must_use]` helper
    `screening::engine::is_auth_failure(e) -> bool`. Other transients stop
    after `TRANSIENT_FAILURE_THRESHOLD = 3` consecutive failures. Both set
    `progress.fatal_error` so the frontend shows a red banner. (b) Transient-
    deferred articles no longer inflate `progress.completed` or `progress.errors`
    (the completion percentage was misleading). New `progress.deferred` counter;
    the frontend renders a muted "N article(s) deferred" notice. (c) The
    orchestrator's timeout error now includes actionable guidance ("try reducing
    batch_size or increasing request_delay_ms") instead of the opaque "timed out."
    Tested in `tests/screening_engine_test.rs` (5 `is_auth_failure` tests).
    **Cancellable inter-batch delay** (v8.5): all three `request_delay_ms`
    sleeps (success path, transient-error path, stage-2 path) are now wrapped
    in the private `delay_or_cancel` helper, which races the sleep against
    `cancel_notify` in a `biased tokio::select!`. Clicking Stop now aborts the
    run within milliseconds during the inter-batch throttle instead of waiting
    the full `request_delay_ms` (commonly 1-10s for rate-limit mitigation).
    The helper MUST NOT use an `if *cancel_token` precondition on the
    `notified()` select branch: tokio::select! skips polling a branch whose
    precondition is false, which prevents `notified()` from registering as a
    waiter, making `notify_waiters()` a no-op and silently losing the cancel
    signal (the sleep runs to completion). Always poll `notified()` and check
    the token inside the branch body. On cancel, the in-memory LLM response
    (if any) is dropped - no partial DB writes, no error marking; articles
    stay unscreened for the next run, matching the cancel-during-LLM-call
    contract. The v8.3 LLM-call wrappers (stage-1 + stage-2) now ALSO use the
    always-poll-notified + check-token-inside pattern (wrapped in a `loop` so
    the spurious-notify path can `continue` without matching the LLM result
    type) - the earlier `if` precondition was wrong: reqwest does NOT yield
    often enough during a slow LLM response, so the cancel signal was lost and
    Stop had no effect mid-LLM-call. Tested in
    `tests/screening_engine_test.rs` (2 `stop_during_request_delay_*` tests:
    success path + transient path; both would hang ~3s without the fix) +
    `tests/screening_two_stage_test.rs` (`stop_during_request_delay_stage2_path`:
    covers the stage-2 delay path at engine.rs ~line 986, asserting cancel
    fires within 200ms AND that the stage-1 decision is preserved when the
    stage-2 response is dropped on cancel). All three tests also assert
    `progress.is_running == false` after cancel (progress-state check) and
    use a tightened `elapsed < 200ms` bound (one scheduler tick, not the
    earlier generous 1500ms).
    **Slow-LLM warning + auto-stop** (v8.6): the `consecutive` transient-failure
    counter alone was insufficient because some batches succeed between
    timeouts (resetting the counter), so a slow LLM limped along without ever
    surfacing a user-visible message. Two new mechanisms address this:
    (a) `ScreeningProgress.warning: Option<String>` — a non-fatal yellow banner
    set after the **1st** timeout ("The LLM is responding slowly… Consider
    reducing batch_size"), cleared on the next successful batch; the frontend
    renders it as `.screening-view__warning-banner` (amber, `warning` icon);
    (b) `total_timeouts: u32` counter + `TOTAL_TIMEOUT_THRESHOLD = 3` — after 3
    total (non-consecutive) timeouts, the run auto-stops with an actionable
    `fatal_error`: "Screening stopped: the LLM timed out N times… Reduce
    batch_size to 1-2 and restart. Already-screened articles are saved." This
    catches the intermittent-timeout pattern where the consecutive counter
    resets between failures. (The v8.6 `batch_size` clamp reduction from
    `clamp(1, 15)` to `clamp(1, 5)` was **reverted** — the clamp silently
    overrode the user's selection on the unproven assumption that large batches
    cause hangs, which masked the real per-batch behavior from the diagnostics.
    `commands/screening.rs` now honors `1..=15` verbatim, matching the frontend
    stepper's `BATCH_MAX`; the orchestrator timeout + auto-stop guards surface
    any genuinely slow provider without baking in a batch-size assumption.)
    `engine.rs` adds `ScreeningMode` (`Abstract`/`Enhanced`/`TwoStage`) + `ScreeningConfig`
    (mode, `enhanced_top_k`, `enhanced_sections`, `two_stage_low`/`high`,
    `chunk_budget_per_article`, optional `max_articles` per-run cap from
    `start_screening(max_articles)`); `run_sync` gains a `config` param. **Enhanced**: per
    article with `has_full_text=1`, retrieves top-K chunks → `rank_chunks_by_criteria` →
    `format_chunks_as_evidence` (pure `#[must_use]`) → attaches as
    `ArticleEntry.full_text_evidence`; one batched LLM call categorized as
    `LlmRequestType::EnhancedScreening`. **Two-stage**: stage 1 abstract-only; borderline
    articles (`two_stage_low <= conf < two_stage_high`, default `[0.4,0.7)`) with full text
    get a second per-article evidence call that overrides stage 1; both passes flow through
    `resolve_decision` and write audit entries (`ai_screen` stage 1, `ai_screen_enhanced`
    stage 2). `ScreeningProgress` gains `stage`/`stage_total` for the progress sub-line.
    `prompt.rs` `ArticleEntry` gains `full_text_evidence: Option<String>`; the prompt emits
    a `## Supporting Evidence from Full Text` block (chunks prefixed `[§Methods]`/`[§Results]`)
    only when `Some` (abstract-mode prompts stay byte-identical). `prompt.rs` `SYSTEM_PROMPT`
    carries a `## Tag and Label Guidelines` section (v6.9): instructs the LLM that
    `suggested_tags` are concise descriptors (≤ 35 chars, lowercase, hyphenated, no
    `inclusion:`/`exclusion:` prefixes - those are for labels). `llm_client.rs` gains a
    non-breaking `send_with_type(system, user, LlmRequestType)` default method (delegates to
    `send`); only `HttpLlmClient` overrides it to route the type through the orchestrator.
    **Tag/label sanitization** (v6.9): `engine.rs` exposes pure `#[must_use]`
    `sanitize_tag_or_label_name(raw, max_len)` + `truncate_at_word_boundary(s, max_len)`.
    The sanitizer strips `inclusion:`/`exclusion:`/`inclusion -`/`exclusion -` prefixes,
    lowercases, replaces spaces/underscores with hyphens, collapses repeated hyphens,
    trims leading/trailing hyphens, and truncates at the last word boundary (never
    mid-word); a single overlong word with no hyphens hard-truncates at the limit.
    `MAX_NEW_TAG_LABEL_LEN = 35` (raised from 30). Both `create_or_match_tag` and
    `create_or_match_label` route through the sanitizer so auto-generated criterion
    labels (`"Inclusion: {text}"`) no longer leak the prefix into the stored name.
    Tested in `tests/screening_engine_test.rs` (12 sanitize + 3 truncate + 3
    create_or_match edge cases) + `tests/screening_prompt_test.rs` (3 system-prompt
    guideline tests).
    `commands/screening.rs` reads mode + params from `app_settings` and runs
    `ensure_chunks_for_full_text_articles(conn, force=false)` inside the spawned
    background task before `run_sync` (NOT in the synchronous IPC handler, so the
    PDF-parse + chunk-write pass does not freeze the UI by holding the DbState
    mutex); the Settings "Rebuild text chunks" button calls the same fn with
    `force=true` so a corrupted/partial/outdated chunk set is repaired. Exposes
    `get_screening_mode`/`set_screening_mode`/`get_full_text_article_count` commands.
    **Diagnostics (v8.7)** — always-on screening instrumentation surfaced to
    diagnose a "hangs with a large corpus + Stop/Pause unresponsive" report.
    No behavioral changes; diagnostics-only. (1) `ScreeningProgress.phase:
    Option<String>` carries the coarse run-phase (`"preparing:translating"` /
    `"preparing:chunking"` / `"screening"` / `"stage2"`); the frontend progress
    bar renders it as the sub-line during prep phases so the user sees
    "Extracting full-text chunks…" instead of a silent 0% freeze. `#[serde(default)]`
    so old payloads still deserialize. (2) `log_diag!` macro (always-on, NOT
    gated on `cfg(debug_assertions)` like `debug_log!`) emits `[screening:diag]`
    lines: phase transitions, per-batch `batch_start`, stage-1/stage-2 cancel
    detection (`llm_call: cancel detected…`), `stop_screening: IPC received`,
    orchestrator `LLM call START/END/TIMEOUT`, and a 5s `HEARTBEAT` (exits on
    `is_running==false || cancel_token==true` so it never leaks past the run).
    Run as `Bango 2>screening.log` and `grep screening:diag`. (3) Phase B
    (chunk backfill) progress callback: `ensure_chunks_for_full_text_articles_with_progress`
    invokes a `ChunkProgressCb` per article; the screening task emits a
    `screening:progress` event + `chunk_progress: done/total` log line per
    article. **The lock pattern is UNCHANGED** — `db.conn.lock()` is still held
    across the whole pass exactly as today; the callback only emits events
    between articles. Layer 2 (deferred) will release the lock per article +
    move PDF parsing to `spawn_blocking`; this diagnostics-only addition
    intentionally preserves the current locking to measure the real production
    behavior first. (4) `connection.rs::lock_conn` times the acquire and emits
    `lock_conn: SLOW acquire ({ms}ms)` when > `SLOW_LOCK_THRESHOLD_MS = 100`;
    this is the single most valuable signal for mutex-starvation hangs (every
    other DB-touching IPC command shows a slow acquire while the chunk pass
    holds the mutex). (5) `translation/wait.rs` emits `translation_wait:
    START/DONE/TIMEOUT` per article (no-op when `auto_translate=false`, the
    opt-in default). Decision table + run instructions in `.worktrees/diagnostic1.md`
    (merged + critiqued; see plan-mode transcript). Tested in
    `tests/screening_engine_test.rs` (3 new tests: `screening_progress_serializes_phase_field`,
    `screening_progress_phase_defaults_none_when_absent`,
    `chunk_progress_callback_fires_per_article`).
    **Always-selectable mode + per-article fallback**: all three modes are
    selectable in Settings regardless of attachments/articles; Enhanced and
    Two-stage evidence retrieval is applied per article only when
    `has_full_text=1` and the run falls back to abstract-only screening
    otherwise (the engine already degrades per-article; the Settings UI no
    longer gates selection on `full_text_article_count >= 1`). The Settings
    card shows a fallback notice (no full text) or an active notice (full text
    present). Migration `v002_wiki_manifest.rs` adds `ai_screen_enhanced` (along with
    `figure_descriptions`) to the `audit_entries.action` CHECK constraint in the
    single audit_entries rebuild (SQLite CHECK constraints can't be ALTERed; uses the
    rename-create-copy-drop pattern). **Stage-2 progress**: every early-exit
    arm in the two-stage loop (evidence filtered out, LLM error, parse mismatch,
    `"error"` decision) updates `ScreeningProgress.stage` so the `X/Y borderline`
    sub-line never stalls. **Token accumulation**: `update_article_after_screening`
    writes `actual_tokens = COALESCE(actual_tokens, 0) + ?` so the stage-2 cost
    adds to (not overwrites) stage-1 for borderline articles. **Accurate enhanced
    audit label**: the evidence-sections label written to the `ai_screen_enhanced`
    audit detail is captured during retrieval (`ArticleEvidence.sections_label`,
    the sections that *actually* matched), not derived from the configured
    allow-list. **Mode-aware token estimation** (Gap 5): `token_estimation::
    worst_case_per_article_tokens` (pure, `#[must_use]`) computes the §4.3
    worst-case footprint per active mode (Abstract = abstract+template;
    Enhanced adds `chunk_budget/4`; Two-stage adds `chunk_budget/4 *
    two_stage_expected_borderline_fraction`); both `get_screening_readiness` and
    `estimate_screening_tokens` route through it so their estimates stay in sync.
    Tested in `tests/screening_prompt_test.rs` (3 evidence-block tests) +
    `tests/screening_two_stage_test.rs` (8 two-stage/enhanced tests: 5 original
    + Gap 3 progress-on-filtered + Gap 6 token-accumulation + Gap 7 accurate
    audit-label) + 1 budget-guard integration test + `tests/token_estimation_test.rs`
    (4 pure-helper cases).
  - **`src-tauri/src/commands/tags.rs`** + **`src-tauri/src/commands/labels.rs`** -
    Tag & Label management commands (v6.9 standard-taxonomy surfacing).
    `tags.rs` owns `STANDARD_STUDY_TAGS` (20 methodology/study-type tags:
    `systematic-review`, `meta-analysis`, `randomized-controlled-trial`, `cohort-study`,
    `case-control-study`, `cross-sectional-study`, `qualitative-study`, `mixed-methods`,
    `pilot-study`, `protocol`, `scoping-review`, `umbrella-review`, `narrative-review`,
    `experimental-study`, `observational-study`, `longitudinal-study`, `prevalence-study`,
    `cost-effectiveness`, `validation-study`, `editorial`) injected into the `suggest_tags`
    prompt as a `## Standard Study-Type Tags` section instructing the LLM to include up
    to 4 when relevant. `labels.rs` owns `STANDARD_WORKFLOW_LABELS` (12 workflow-state
    labels: `priority-read`, `strong-methodology`, `weak-methodology`, `needs-full-text`,
    `disputed`, `key-paper`, `borderline`, `duplicate-suspect`, `excluded-by-criteria`,
    `included-by-criteria`, `needs-discussion`, `flagged`) injected into the
    `suggest_labels` prompt similarly. All standard entries are pre-validated to pass the
    35-char `sanitize_tag_or_label_name` gate so the backend sanitizer never silently
    truncates them. Both prompts also reinforce the ≤ 35-char + no-prefix rules so the
    standalone suggestion path stays consistent with the screening-time path.
    Frontend `tag-label-management.vue` (v6.9): double-click any tag/label chip to edit
    in place (same affordance as the criteria editor); `nextTick` auto-focus + select
    the input on edit-start, `@blur` commits, `Escape` cancels.
  - **`src-tauri/src/db/app_settings_repo.rs`** - key/value `app_settings` store. Holds
    `storage_root` (Bango documents root; `fulltext/`, `ris/`, `wiki-root/` derive from it
    as subdirectories; lazy-migrated from the legacy `fulltext_storage_dir` key by
    `get_storage_root`, which strips a trailing `fulltext` segment to derive the root),
    `flag_premium`, `biblio_needs_refresh` (the bibliometric
    staleness flag), `wiki_needs_refresh` (the LLM Wiki staleness flag),
    `auto_translate` (experimental toggle for translating non-English articles to
    English during AI processing; DB-backed unlike the sibling localStorage AI
    Summary toggles; **default `false` (opt-in)** - decision (a) flipped it from
    `true` so imports do not silently trigger background translation + LLM
    cost; the user must enable it explicitly in Settings; absent/garbage value
    falls back to the default), and
    `summary_evidence_mode` (project-wide literature-review evidence enrichment;
    `abstract_only` default | `with_summary_facts` - see `commands/summary.rs::generate_summary`
    + the `format_ai_summary_as_evidence` pure helper in `summary/prompt.rs`), and
    `screening_custom_logic` (optional free-text combinatorial screening rules -
    AND/OR gates, hard exclusions, conditional inclusion - authored on the
    Criteria screen Section 4 "Custom Screening Instructions" and injected into
    every screening prompt as a `## Custom Screening Instructions` section after
    `## Priority Rules`; references criteria by their **global number** so the
    LLM, the user, and the reasoning all mean the same thing by "criterion 3";
    empty/absent → no section emitted, byte-identical to pre-feature prompts;
    `commands::criteria::check_rules` runs an LLM consistency review over the
    whole ruleset incl. custom rules; the screening prompt now numbers
    inclusion `1..N` then exclusion continues `N+1..N+M` via
    `CriterionEntry.global_number` so "criterion 11" is unambiguous),
    `openalex_api_key` (AES-256-GCM encrypted; optional; raises rate-limit tier
    from 10 to 100 req/s; set/read via `openalex::mod.rs::get_api_key` /
    `set_api_key`; **deliberately excluded from `PROJECT_PORTABLE_SETTINGS`** -
    API key, never exported),
    `openalex_mailto` (plaintext string; user email for the OpenAlex polite
    pool; sent as `mailto` query param on every request; **included in
    `PROJECT_PORTABLE_SETTINGS`** - non-secret user preference; if unset, a
    Bango app default `"research@bango.app"` is used),
    `openalex_retrieve_references` (plaintext boolean `"true"`/`"false"`;
    gates the OpenAlex Reference + Citation Harvest: when enabled, importing
    from OpenAlex batch-fetches both outgoing `referenced_works` and incoming
    `cites:` citations and populates `reference_papers` +
    `article_reference_links`; defaults to `false`; **included in
    `PROJECT_PORTABLE_SETTINGS`** - non-secret user preference).
    `mark_biblio_needs_refresh(conn)` is called by every mutation that
    changes data bibliometrics depends on (RIS/BibTeX import in `commands/import.rs`,
    project backup restore in `commands/export_cmd::import_project_backup`,
    reference/citation import + CR extraction + reference promotion in
    `commands/references.rs`, tag/label/status/override/bulk edits in `commands/articles.rs`,
    and AI screening completion in `commands/screening.rs`). `clear_biblio_needs_refresh`
    runs only after `biblio_normalize` commits successfully; `get_biblio_needs_refresh`
    powers the frontend `biblio_get_needs_refresh` command. Absent key = fresh (false).
    `mark_wiki_needs_refresh(conn)` is called by every mutation that changes the Wiki's
    content sources (`full_text` attach/delete in `commands/full_text.rs`, AI-summary
    regen in `commands/summary.rs::generate_article_ai_summary`) plus the same corpus
    mutations that set the biblio flag (RIS/BibTeX import, project backup restore,
    reference/citation import, tag/label/status/override/bulk edits, AI screening
    completion). `clear_wiki_needs_refresh` runs only after `wiki_ingest`/`wiki_rebuild`
    commits; `get_wiki_needs_refresh` powers the frontend `wiki_get_needs_refresh`
    command that drives the `autoIngestIfStale()` flow in `wiki-view.vue`. Absent key =
    fresh (false). Tested in `wiki_full_text_refresh_test.rs`.
  - **`src-tauri/src/wiki/`** - LLM Wiki knowledge-base module (all phases complete).
    Generates and maintains a local-first Obsidian-style Markdown knowledge base from the
    `included` article corpus. Modules: `storage.rs` (resolves `wiki-root/`, scaffolds
    `raw/`, `wiki/{concepts,authors,methods,synthesis}/`, `templates/`, `AGENTS.md`,
    `log.md`), `agents_contract.rs` (ingest + lint rules contract), `templates.rs` (page
    templates), `frontmatter.rs` (dependency-free YAML parser/serializer),
    `raw_export.rs` (included-article export + user-file extraction for PDF/TXT/HTML/etc),
    `fts.rs` (FTS5 BM25 search index + **two-tier external-edit drift detection**),
    `ingest/` (directory module: LLM page generation - prompt builder,
    `<!-- PAGE:slug -->` response parser, page writer, FTS5 rebuild, **parallel
    chunked ingest**; submodules: `mod.rs` core pipeline + re-exports,
    `batching.rs` chunked/parallel batch building + `run_chunked_ingest`,
    `consolidation.rs` deterministic dedup + `[[wikilink]]` rewrite,
    `authors.rs` Phase 1 author manifest + pre-seed, `synthesis.rs` Phase 2
    synthesis pre-seed, `concepts.rs` Phase 3 concept hub pre-seed,
    `sources.rs` Layer 1 external-document source pages, `slugs.rs` shared
    `squeeze_slug` helper. Inline tests extracted to
    `tests/wiki_ingest_test.rs` per `docs/CLAUDE.md` §Testing), `engine.rs`
    (deterministic lint + `build_graph` for link graph
    visualization), `chat.rs` (token-budgeted RAG chat over FTS5 index; self-heals the
    FTS table via `fts::ensure_index_populated` when the index is empty OR its row count
    mismatches the number of `.md` pages on disk).
    **Parallel chunked ingest** (`ingest/batching.rs`): `wiki_ingest`, `wiki_rebuild`, and
    `wiki_export_and_ingest` no longer make one monolithic LLM call. They split raw
    sources into batches sized to `config.context_window_tokens * 0.4` (input budget;
    remainder is available for output pages), dispatch all batches concurrently via a
    `tokio::task::JoinSet` (bounded by the orchestrator's `max_concurrent_requests`
    semaphore), and emit `wiki:progress` on every batch completion so the progress bar
    moves smoothly across the 25-95% range. Each batch carries a compact full-source
    index (title + slug) so the model can `[[link]]` across batches without sequential
    slug-forwarding. Per-batch failures are tolerated (recorded in `report.errors`;
    other batches still write). Key types: `IngestBatch`, `IngestLlmSender` (injectable
    trait; production `OrchestratorIngestSender`, test `FakeSender`),
    `build_ingest_prompt_batches`, `run_chunked_ingest`. (`write_pages_from_response`
    remains for the async write-and-index path; the legacy single-call
    `build_ingest_prompt` was deleted in the Tier B2 hallucination-reduction
    pass - the batch path now covers all production callers.)
    **Multi-batch consolidation** (gated on `batches.len() > 1`): when the corpus
    splits into multiple parallel batches, independent batches often produce
    near-duplicate pages for the same concept (`childhood-obesity` vs
    `obesity-childhood`). To prevent fragmentation, `run_chunked_ingest` collects all
    `ParsedPage`s across batches, runs a **deterministic** `consolidate_pages` pass
    (no LLM merge calls), rewrites inbound `[[wikilinks]]` to canonical slugs via
    `rewrite_page_links`, then writes the consolidated set. Detection: two
    same-type (non-author) pages merge when (a) slugs match case-insensitively, OR
    (b) stemmed-token Jaccard similarity of slugs >= `DEDUP_JACCARD_THRESHOLD` (0.5),
    OR (c) they share >= `DEDUP_SHARED_SOURCES_MIN` (2) `source_articles`. Merge is
    lossless: the duplicate body is appended under `## Additional perspectives`;
    `source_articles` + `tags` are unioned. Author pages are pre-seeded and excluded
    from merging. `AuthorManifest` + `preseed_authors` + `build_author_manifest`
    derive canonical author slugs from `biblio_authors` (populated by running
    `run_full_normalization` first - the full 8-step bibliometric pipeline
    extracted into a pure `pub fn run_full_normalization(conn)` in
    `biblio_repo/normalization.rs` and shared by both `biblio_normalize` and the
    wiki ingest path, so there is no raw-frontmatter fallback) and inject a
    "DO NOT create author pages" section into every batch prompt so batches link
    to the same author slugs instead of inventing their own. Each pre-seeded
    author page is a rich hub: metrics line (h-index, total citations,
    first-author count, papers/year), Publications list with `[^art-id]`
    footnotes + real `source_articles` frontmatter, Research Areas
    (deduplicated keywords aggregated from `biblio_article_terms`), and
    Frequent Collaborators (`[[author-slug]]` links derived from shared-paper
    counts).
    Single-batch runs (`batches.len() == 1`) skip all consolidation - the LLM sees
    all sources at once and produces a self-consistent page set, so the manifest,
    pre-seed, dedup, and link rewrite are zero-cost no-ops.
    **Deterministic 5-layer pre-seed matrix** (`build_batches_with_manifest` in
    `commands/wiki_cmd.rs`, runs unconditionally before the LLM on every
    single-batch AND multi-batch run): (1) `preseed_authors` writes rich author
    pages from `biblio_authors` (metrics, publications, research areas,
    collaborators); (2) `preseed_synthesis_from_ai_summaries` writes one
    `wiki/synthesis/{article_id}.md` per included article that has a
    `full_text_ai_summary` JSON blob - slug = article UUID (so `[[uuid]]` links
    resolve), body = `summary_150_250_words` digest + `key_insights` bullets,
    `tags` = keyword-derived `[[concept-slug]]` candidates; (3)
    `preseed_concept_hubs` writes top-25 `wiki/concepts/{term-slug}.md` hub
    pages from `biblio_terms`, each linking to its articles (`[[uuid]]`) +
    co-occurring concepts; (4) **`preseed_methods`** writes top-25
    `wiki/methods/{method-slug}.md` hub pages from AI-summary `study_design`
    (when present) with a `biblio_terms` fallback for abstracts-only corpora;
    a curated study-design lexicon (`STUDY_DESIGN_LEXICON` in
    `ingest/methods.rs`) canonicalizes synonyms (e.g. "RCT" →
    `randomized-controlled-trial`) so non-methodological terms are filtered.
    When the pre-seed writes >=1 method page, the batch directive tells the LLM
    methods are handled (link, don't duplicate); when it writes 0 pages, the
    directive flips to "methods NOT pre-seeded - create them" + the focus list
    always asks the LLM for METHOD pages so `wiki/methods/` is never empty;
    (5) **`preseed_document_source_pages`** writes one
    `wiki/sources/{user-slug}.md` per user-uploaded document (Add Documents →
    PDF/TXT/web, identified by `source_kind: user_*`) so external documents get
    a first-class wiki node and `[^art-user-slug]` / `[[user-slug]]` citations
    resolve to a navigable page instead of "Page not found". This layer mirrors
    the article→synthesis symmetry: every raw source has a corresponding wiki
    node. All five respect `status: reviewed` (user-edited) pages. Together they
    form a connected graph backbone (author ↔ synthesis ↔ concept ↔ method ↔
    source) that exists before the LLM runs, so the wiki is never missing
    author/synthesis/concept/method/source pages regardless of which LLM model
    is used. Tested in `wiki_deterministic_test.rs` + `wiki_methods_preseed_test.rs`.
    Design + phases 4-5 (LLM prompt narrowing, `concepts` field in AI summary
    schema) in `.worktrees/DONOTUSE/wiki-improvement-plan.md`; external-document
    ingestion + linking design in `.worktrees/DONOTUSE/wiki-improvement-plan2.md`;
    hallucination-reduction plan (methods pre-seed + grounding gate + prompt
    cleanup) in `.worktrees/wiki-implementation.md`.
    **Tier A1 grounding gate** (`engine.rs` `LintKind::UngroundedPage`): after
    every ingest, `run_chunked_ingest` runs `engine::lint` and counts pages
    failing the ERROR-level provenance check (LLM-generated
    concept/method/synthesis pages missing `source_articles` frontmatter).
    Author/source pages are exempt (pre-seeded with a different provenance
    shape). The WARNING-level check (missing `[^art-]` citations in the body)
    surfaces via the standalone `wiki_lint` command. The error count is appended
    to `IngestReport.errors` so the UI + diagnostics can flag ungrounded pages.
    Tested in `wiki_grounding_test.rs`.
    **Temperature inheritance**: wiki ingest inherits the global
    `LlmConfig.temperature` (default 0.2, suitable for deterministic KB
    generation). There is no per-`LlmRequestType` override; users targeting
    maximal determinism should set it to `0` in Settings and rely on
    `skip_temperature` for incompatible models (the orchestrator + `client.rs`
    own the `skip_temperature` gate + retry-without-temperature path).
    `commands/wiki_cmd.rs` exposes
    all Tauri commands: `wiki_get_status`, `wiki_init`, `wiki_export_raw`,
    `wiki_add_raw_file`, `wiki_list_raw_files`, `wiki_search`, `wiki_lint`,
    `wiki_get_page`, `wiki_update_page`, `wiki_delete_page`, `wiki_delete_wiki`,
    `wiki_chat`, `wiki_get_graph`, `wiki_ingest`, `wiki_list_pages`, `wiki_list_sources`,
    `wiki_rebuild` (one-click full pipeline: scaffold + export + ingest + FTS5, emits
    `wiki:progress` events), `wiki_export_and_ingest` (export + ingest after Add Documents),
    and `wiki_check_for_updates`, plus `wiki_export_site` (static-site zip export: the
    frontend renders all HTML via `renderWikiMarkdown(staticMode)` + depth-aware
    `slugToHref`/`artIdToHref` resolvers and passes a `SiteExportBundle` to this command,
    which writes the staging dir, copies the wiki + user-doc Markdown tree, zips, and
    moves the zip to the frontend-chosen path; no `blocking_pick_file` in the backend).
    `wiki_search` rebuilds the FTS index if empty;
    `wiki_update_page` / `wiki_delete_page` rebuild it on every edit/delete so chat + search
    stay in sync with user changes (both use `rebuild_index_with_manifest` so the drift
    manifest stays in sync too).
    **External-edit drift detection** (`wiki_check_for_updates`, async): detects when
    external programs edit `wiki/**/*.md` files and re-indexes them transparently WITHOUT
    re-running the LLM ingest. Runs entirely on the tokio runtime - all file reads + per-file
    SHA-256 hashing happen lock-free; the `DbState` mutex is held only for millisecond-scale
    SQLite writes (FTS5 rebuild + manifest rewrite + dir-hash update). Two tiers keep the
    common case cheap: tier-1 is a stat-only directory fingerprint (`wiki_dir_hash` in
    `app_settings`) that short-circuits when nothing changed; tier-2 is the
    `wiki_index_manifest` table (per-file content hashes) that distinguishes real edits from
    `touch`. Triggers: Wiki view `onMounted`, Chat view `onMounted` (when wiki-ready), and
    the toolbar "Check for Updates" button (manual, bypasses the 30s debounce in
    `use-wiki.ts`). Emits `wiki:files-changed` on rebuild. Toast UX: "Checking for Wiki
    updates..." -> "Wiki updated: N pages re-indexed." / "Wiki is up to date."
    **Self-healing init guard**: `ensure_initialized(root)` writes `AGENTS.md` when
    missing; called at the top of `wiki_init`, `wiki_ingest`, `wiki_rebuild`, and
    `wiki_export_and_ingest` so an uninitialized wiki transparently recovers instead of
    leaving generated pages invisible behind the wiki-view "Initialize" empty-state gate
    (`initialized` is `AGENTS.md`-presence-based). Idempotent: never overwrites an existing
    `AGENTS.md`. Tested in `wiki_ensure_initialized_test.rs`.
    The `wiki_needs_refresh` flag triple lives in `app_settings_repo.rs`; cleared after
    `wiki_ingest`/`wiki_rebuild` commits. Frontend: `wiki-view.vue` (sidebar + viewer +
    editor + graph + article detail slide-over), `wiki-toolbar.vue` (Re-scaffold, Add
    Documents, Lint, Delete Wiki, progress bar, and a single-purpose Chat button that
    deep-links into `/chat` with Wiki mode pre-enabled - gated on LLM configured +
    wiki initialized with pages), `wiki-page-viewer.vue` (Markdown render via
    the shared `src/utils/wiki-markdown.ts` - `[[wikilink]]` + `[^art-id]` source ref
    resolution), `wiki-page-editor.vue` (split-pane editor), `wiki-graph-panel.vue`
    (sigma + ForceAtlas2 graph). Node labels truncate to 25 chars + ellipsis
    on the canvas; a Vue hover tooltip (mirroring `citation-network-graph.vue`)
    shows the full title + page `summary` + inbound/outbound counts via
    sigma's `moveBody` event. The `GraphNode.summary` field is populated from
    frontmatter by `engine::build_graph`. Composable: `use-wiki.ts`.
    Design and phasing: `.worktrees/DONOTUSE/llmwiki-plan.md`.
    **Chat-with-Wiki integration**: `useChatStore.source: 'articles'|'wiki'` (mutually
    exclusive) switches the `/chat` view between `send_chat_message` (article RAG) and
    `wiki_chat` (BM25 FTS5 RAG). A Wiki toggle button (icon `local_library`) in `chat-view.vue`
    sits right of the `(+)` icon, visible only when `wikiReady` (wiki initialized AND
    `pageCount > 0`). Wiki-sourced assistant bubbles render via `src/utils/wiki-markdown.ts`
    so `[[slug]]` citations become clickable links that open a right-side Wiki reader
    slide-over (`WikiPageViewer` with a back-stack). The wiki-toolbar owns a Chat
    button that deep-links into `/chat` with `chatStore.setWikiReady(true)` +
    `chatStore.setSource('wiki')` pre-applied, so the user lands in Wiki-mode RAG
    chat in one click (gated on LLM configured + wiki initialized with pages).
  - **`src-tauri/src/openalex/`** - OpenAlex catalog search integration (§8.5).
    Modules: `mod.rs` (types: `OpenAlexWork`, `OpenAlexSearchResponse`,
    `OpenAlexFilters`, `OpenAlexResultItem` + `get_api_key`/`set_api_key`/
    `get_mailto` helpers; `OpenAlexWork` carries `#[serde(default)]` on
    `cited_by_count`, `keywords`, and `authorships` so the struct deserializes
    from any API response subset - the harvest `select` omits
    `cited_by_count`/`keywords`, and without the default serde fails with
    "missing field" and silently drops the entire reference/citation harvest),
    `client.rs` (HTTP client with 429 retry + `mailto`/`api_key` injection +
    100ms batch pause + `download_pdf` + `fetch_citing_works` for cited_by
    direction; `download_pdf` sends browser-like headers (`User-Agent`,
    `Accept`, `Accept-Language`, `Referer`, `Sec-Fetch-*`) so publishers
    (MDPI, Elsevier, Springer) that 403 on the minimal `Bango/2.0` UA serve
    the PDF instead of a block page; error messages include the URL + HTTP
    status for diagnostics), `mapping.rs` (pure helpers:
    `reconstruct_abstract`, `truncate_snippet`, `map_work_to_new_article`,
    `map_works_to_new_articles`, `map_work_to_reference_paper`), `search.rs`
    (`build_search_url` with percent-encoding), `smart_search.rs` (LLM-generated
    Boolean query from aims + criteria via `LlmRequestType::OpenAlexSmartSearch`),
    `reference_harvest.rs` (batch-fetch both outgoing `referenced_works` and
    incoming `cites:` citations when `openalex_retrieve_references` is enabled;
    inserts as `reference_papers` + `article_reference_links` with
    `ReferenceType::Reference` / `ReferenceType::Citation`; harvest errors
    logged to the **article's** audit trail via `log_harvest_error` helper
    which writes `action = "error"` with the article_id so failures surface
    in the Audit Timeline, not just the generic Diagnostics feed). Commands
    live in `commands/openalex.rs`: `search_openalex`,
    `import_openalex_articles` (3-phase: sync DB insert + async ref/citation
    harvest + async PDF download with auto AI summary; accepts
    `auto_summarize` + `include_section_summaries` params so the frontend can
    pass the `bango-full-text-summaries` / `bango-section-summaries`
    localStorage flags - the backend cannot read localStorage; PDF download
    + attach + extraction errors are logged to the article's audit trail via
    `log_article_error` helper, NOT `log_error_best_effort` which writes
    `article_id = NULL` and hides them from the Audit Timeline), `check_dois_in_library`,
    `smart_search_openalex`, `get_openalex_settings` / `set_openalex_settings`,
    `download_and_attach_openalex_pdf`. Import reuses the existing
    `insert_articles_batch` -> `classify_imported_articles` ->
    `resolve_journal_links` pipeline (parity with RIS/BibTeX). No migration
    needed (`'import'` already in `audit_entries.action` CHECK). Tested in
    `tests/openalex_mapping_test.rs` (11 tests incl.
    `deserialize_harvest_response_missing_fields`) +
    `tests/openalex_search_test.rs` (5 tests) +
    `tests/openalex_import_test.rs` (5 tests + 1 ignored Tier 2 stub) +
    `tests/openalex_smart_search_test.rs` (5 tests). Capabilities
    (`src-tauri/capabilities/default.json`) allow `https://**` + `http://**`
    for `opener:allow-open-url` so DOI/PDF/OA links open from the Search
    detail panel (publisher domains are not predictable, so the allow-list
    cannot be a fixed domain set).
  - **`src-tauri/src/db/biblio_repo/`** - bibliometric repos (`kpis`, `authors`,
    `networks`, `terms`, `institutions`, `normalization`, `productivity`). Contract:
    `get_biblio_kpis` returns `BiblioKpis` including `journal_distribution:
    Vec<JournalYearData>` (canonical titles via `journal_index` LEFT JOIN, fallback
    `UPPER(TRIM(journal))`). `productivity.rs` exposes `get_author_rankings`,
    `get_author_detail`, `get_author_productivity_kpis` - author-level h-index, i10,
    g-index, first/last/solo counts scoped to included articles. `networks/` is a directory
    module (split from the former monolithic `networks.rs`) with one file per network type:
    `persistence.rs` (generic network CRUD: save/load/delete nodes & edges), `labels.rs`
    (shared `format_paper_label` helper), `coauthors.rs` (full + fractional edge building),
    `citations.rs` (directed citation edges + unmatched-leaf nodes), `keywords.rs`
    (keyword co-occurrence), and `cocitation.rs` (on-demand co-citation computation with 4
    normalization modes: Raw, Cosine, Jaccard, Pearson; `CocitationScope` = included/all
    articles). `mod.rs` re-exports the public API unchanged.
  - **`src-tauri/src/db/article_repo.rs`** - article CRUD + the `ArticleQuery` filter
    contract used by `query_articles` (the `query_articles` Tauri command feeds the Article
    list table). `ArticleQuery` carries four tag/label filter vectors: `tags` +
    `excluded_tags`, `labels` + `excluded_labels`. The inclusion vectors (`tags`/`labels`)
    emit `articles.id IN (SELECT ...)` clauses (article must have the tag/label); the
    exclusion vectors (`excluded_tags`/`excluded_labels`) emit `articles.id NOT IN (SELECT
    ...)` clauses (article must NOT have the tag/label) so the Article list filter panel can
    toggle a pill between inclusion and NOT-exclusion. All four are `#[serde(default)]` so
    old callers omitting them still deserialize. An empty exclusion vector filters nothing.
    Comparison is `LOWER()`-based (case-insensitive) for both directions. It also carries
    two DOI filter fields: `doi: Option<String>` (case-insensitive partial match, emits
    `LOWER(doi) LIKE '%...%'`; empty/None filters nothing) and `doi_empty: bool` (when true,
    emits `doi IS NULL OR doi = ''` for the "find articles missing a DOI" data-cleanup
    workflow). The two are mutually exclusive: `doi_empty` wins if both are set, avoiding
    contradictory SQL (`doi LIKE '%x%' AND doi IS NULL` would return zero rows). Both are
    `#[serde(default)]`. Tested in `tests/article_query_test.rs` (5 NOT-filter tests:
    excluded tag, excluded label, case-insensitive exclusion, inclusion+exclusion combine,
    empty-exclusion no-op; + 5 DOI tests: partial match, empty-only, empty-wins-over-text,
    case-insensitive partial, combines-with-status).
    Frontend mirror: `ArticleFilter`/`ArticleQuery` in
    `src/composables/use-article-search.ts` carry `excludedTags`/`excludedLabels` +
    `doiText`/`doiEmpty`;
    `src/components/article-filter-panel.vue` renders excluded pills with a bold `NOT:`
    prefix + strike-through on the name, toggled by clicking the pill body (the `x` button
    removes entirely via `removeExcludedTag`/`removeExcludedLabel`), and a DOI text input
    paired with an "Only no DOI" checkbox that disables the input when checked.
  - **`src-tauri/src/db/journal_repo.rs`** - journal_index lookup/match (`resolve_journal_id`,
    `match_journal`, `get_journal_info`). `articles.journal_index_id` is populated on import
    and refreshable via the `rematch_journals` command.
  - **`src-tauri/src/db/connection.rs`** - holds `DbState` (`conn: Mutex<Connection>`) and the
    shared `lock_conn(conn_mutex) -> Result<MutexGuard<'_, Connection>, AppError>` helper that
    maps `Mutex::lock()` poison failures to `AppError::LockPoisoned` (not `AppError::Database`).
    Every command handler and engine that locks `DbState.conn` MUST route through `lock_conn`
    instead of inlining `.lock().map_err(...)` so poison errors stay correctly categorized as
    application-state errors and the error-mapping boilerplate is not duplicated. The private
    `lock_conn` in `commands/wiki_cmd.rs` and `lock_db` in `translation/engine.rs` were removed
    in favor of this shared helper. Tested in `tests/lock_poison_test.rs`.
  - **`src-tauri/src/db/schema_check.rs`** + **`rebuild.rs`** - startup legacy-DB detection
    and schema rebuild. `check_schema` classifies a live DB as `Current` / `Legacy` / `FreshDb`
    via `sqlite_master` (the old and new v1 migrations both set `user_version=1`, so the pragma
    cannot be trusted). `rebuild_schema` is the shared drop-all-tables (preserving
    `journal_index`) + reset `user_version=0` + re-run migrations helper used by both
    `commands::export_cmd::reset_project` and the legacy upgrade path. `DROP_TABLES` includes
    the lazily-created `wiki_pages_fts` FTS5 virtual table (it is not created by migrations);
    it self-heals via `fts::ensure_index_populated` on the next wiki read. Also dropped: the `wiki_index_manifest` drift-detection cache (created by migration v002), which self-heals via `wiki_check_for_updates`. `reset_project`
    additionally deletes the entire on-disk `wiki-root/` directory (resolved BEFORE the schema
    rebuild, while `app_settings` still holds the path config); wiki deletion is non-fatal.
  - **`src-tauri/src/commands/startup.rs`** - exposes `get_startup_status` and
    `perform_legacy_upgrade` (one-shot: `export_legacy_project` -> write backup to
    `app_data_dir` -> `rebuild_schema` -> journal reload -> `import_project`; backup file
    is never deleted). **Loop-safety**: a webview `window.location.reload()` runs in the
    same Rust process, so managed state is not recomputed. To prevent an endless reload
    loop after a successful upgrade, `get_startup_status` re-probes the LIVE schema on
    every call (falling back to the setup-time snapshot only if the live probe errors),
    and `perform_legacy_upgrade` updates the managed `StartupStatus` snapshot (now a
    `Mutex<SchemaStatus>`) post-success. Pure decision logic lives in
    `legacy_upgrade_needed(live, fallback)`; the frontend adds a third
    sessionStorage-based guard in `use-startup-upgrade.ts`.
  - **`src-tauri/src/export/project.rs`** - `ProjectBackup` serialize/deserialize. Exports
    source tables (aims, criteria, articles, tags, labels, article_tags/labels, audit,
    reference_papers, article_reference_links, llm_config) plus a curated
    project-portable subset of `app_settings` (see `app_settings_repo.rs` entry above:
    `screening_custom_logic`, `summary_evidence_mode`, `auto_translate`, screening-mode +
    enhanced/two-stage params). The `appSettings` field is `#[serde(default)]` so old
    backups without it import cleanly. Only allowlisted keys are exported/imported
    (defense-in-depth via `is_project_portable`); machine-local state (`storage_root`,
    `flag_premium`, the `*_needs_refresh` flags, `wiki_dir_hash`, `openalex_api_key`) is
    deliberately excluded. **DOX rule: any change to `app_settings` keys (adding, renaming,
    or removing a setting) MUST trigger a review of `PROJECT_PORTABLE_SETTINGS` in
    `app_settings_repo.rs` to decide whether the new key should travel with project backups.
    Secrets (API keys, encrypted values) must NEVER be added to the allowlist.**
    The 9 `biblio_*` tables are NOT exported - they are dynamically generated by
    `biblio_normalize` and would bloat backups and trigger UNIQUE constraint violations on
    import. `article_chunks` is also NOT exported - it is regenerated at attach time; the
    import purge sequence explicitly `DELETE FROM article_chunks` before wiping `articles`
    because `PRAGMA foreign_keys=OFF` during import prevents the `ON DELETE CASCADE` from
    firing. After import, `mark_biblio_needs_refresh` ensures the frontend auto-regenerates
    the biblio tables. The import code uses `INSERT OR IGNORE` + ID-remap maps for
    `reference_papers`, `biblio_authors`, `biblio_institutions`, and `biblio_terms` (all have
    UNIQUE constraints) to handle older backups that may still contain biblio data.
  - **`src-tauri/src/export/legacy_project.rs`** - reads the old single-table
    `article_references` schema and emits a current-format `ProjectBackup` JSON, deduplicating
    rows into `reference_papers` (by DOI -> title+authors+year) + `article_reference_links`.
  - **`src-tauri/tests/`** - Rust integration tests. Inline `#[cfg(test)] mod tests`
    blocks are extracted here to keep source files compact (helpers tested externally
    are `pub`). Repository/KPI tests live in `biblio_repo_tests.rs` (in-memory SQLite
    via `run_migrations`). Network builder & serializer unit tests (network CRUD,
    co-author/keyword JSON, and the full co-citation suite) live in
    `biblio_networks_test.rs`. Unit-test extractions: `biblio_normalizer_test.rs`,
    `biblio_models_test.rs`, `bibtex_parser_test.rs`, `bibtex_converter_test.rs`,
    `cr_parser_test.rs`, `doi_test.rs`, `n1_parser_test.rs`,
    `screening_engine_test.rs`, `pdf_extract_test.rs`, `browser_test.rs`. Co-citation
    integration tests against RIS fixtures live in `cocitation_data_test.rs`.
    `biblio_needs_refresh_test.rs` covers the staleness-flag round-trip (mark/clear/
    absent-key default). `auto_translate_test.rs` covers the experimental
    auto-translate `app_settings` toggle round-trip (default-true absent key, set
    false/true round-trips, garbage value falls back to default).
    `wiki_full_text_refresh_test.rs` covers the wiki staleness-flag
    pairing with content-source mutations (`full_text` attach/delete, AI-summary regen)
    plus the wiki-flag round-trip. `legacy_upgrade_test.rs` covers the full legacy upgrade round-trip
    (legacy article_references -> backup -> rebuild -> import) plus the
    `legacy_upgrade_needed(live, fallback)` pure decision function (live-probe-wins and
    snapshot-fallback branches). `reset_project_test.rs` covers `reset_project_inner`
    (Delete All Data): verifies the on-disk `wiki-root/` directory is deleted, `app_settings`
    is cleared after rebuild, and the reset succeeds even when the wiki root is missing.
    `wiki_consolidation_test.rs` covers the multi-batch consolidation pipeline
    (cross-batch dup merge + link rewrite + single-batch skip + unrelated-page
    preservation) using injectable `IngestLlmSender` fakes. `wiki_index_drift_test.rs`
    covers the two-tier external-edit drift detection (external body edit -> rebuild,
    `touch` -> dir-hash update only, page add/delete -> path-set drift, internal edit
    via `rebuild_index_with_manifest` -> no false-positive, empty-wiki baseline clear,
    order-independent fingerprint, manifest round-trip).
    `sections_test.rs` covers `utils::sections::classify_sections` (markdown /
    numbered / bare-keyword heading detection, references exclusion, Text fallback,
    word-count, Materials-and-Methods classification) + 3 `#[ignore]`d real-PDF
    end-to-end tests against committed OA fixtures: `plos-med-1004371.pdf`
    (Cobiac 2024, CC-BY, 7 sections / 21 chunks), `pone-0285956.pdf` (Oakland SSB
    tax, CC-BY, 5 sections / 17 chunks), and `demo-vfs-2022-pid-69753.pdf`
    (lopdf-fallback space-degenerate regression). `section_summary_prompt_test.rs`
    (T1.3, 14 tests) covers the section-aware AI summary prompt helpers
    (`filter_high_value_sections`, `build_section_context`), the
    `SectionKind::label()` display strings, the
    `ARTICLE_SUMMARY_WITH_SECTIONS_SYSTEM_PROMPT` content guard (schema keys +
    delimiter format), and JSON backward-compat (v1 blobs without
    `section_summaries` + v2 blobs with `section_summaries` both parse through
    `serde_json::Value` as the command stores `parsed.to_string()`).
    `chunking_test.rs` has inline tests in `utils/chunking.rs` (9 tests: empty input,
    short section, references skip, long-section sentence split, tiny-tail merge,
    Text label, Heading label, contiguous chunk_index, empty-body skip) + a standalone
    `src-tauri/tests/chunking_test.rs` (Tier 2 Phase 1 atomic Table/Figure tests +
    `proptest` invariants: word-count bounds excluding atomic Table/Figure, contiguous
    `chunk_index`).
  - **`src-tauri/src/utils/sections.rs`** - section-aware text classification (T1.1 +
    Tier 2 Phase 1). `classify_sections(text)` splits flat extracted text into
    `Vec<Section>` by detecting heading lines (markdown `##`, numbered `2.1 Study
    Design`, bare keyword `METHODS`). `SectionKind` enum: `Heading, Abstract,
    Introduction, Methods, Results, Discussion, Conclusion, References, Table, Figure,
    Text` (Table/Figure added in T2 Phase 1 for caption/table extraction).
    `SectionKind::label()` returns the stable display string. Tier 2 Phase 1 added:
    `extract_captions(text)` (multi-line Figure/Table caption extraction via
    `CAPTION_START_RE` with greedy continuation), `detect_markdown_tables(text)` (pipe +
    whitespace-aligned table detection returning GFM tables + `<!-- TABLE:N -->`
    placeholders), `extract_sections_with_tables(text)` (composer that keeps
    `classify_sections` untouched). Constants: `COLUMN_ALIGN_TOLERANCE=2`,
    `MIN_TABLE_LINES=2`. `extract_sections(path)` is the I/O wrapper. Pure functions
    (`#[must_use]`); consumed by T1.2 `chunking.rs`, T1.3 `summary::prompt`, T2.4
    `raw_export::structure_full_text`, and T3.1 `attach_full_text` chunk storage.
    Tier 2 proptest invariants live in `src-tauri/tests/sections_test.rs` (page-spanning
    break) + `src-tauri/tests/chunking_test.rs` (word-count bounds + contiguous index).
  - **`src-tauri/src/utils/chunking.rs`** - semantic chunking (T1.2 + Tier 2 Phase 1).
    `chunk_sections(sections, target_words)` walks `Section`s and emits `Vec<Chunk>`
    bounded by `DEFAULT_CHUNK_WORDS=512` / `MIN_CHUNK_WORDS=100` / `MAX_CHUNK_WORDS=1200`.
    Splits long sections at sentence boundaries; merges tiny tails (now MAX-guarded so a
    near-MAX chunk + tiny tail cannot exceed the hard cap); skips `References` entirely;
    carries section provenance (`Some("Methods")`) so FTS5 chunk rows + chat citations
    can render `(§Methods)`. **Atomic Table/Figure arm** (T2 Phase 1): `SectionKind::Table`
    / `Figure` sections are emitted as a single chunk regardless of `MAX_CHUNK_WORDS` so
    GFM tables survive intact into the FTS index. Pure functions (`#[must_use]`).
    Consumed by `wiki::fts` (chunk-emission) and T3.1 `attach_full_text` chunk storage.
    Property-based tests (`proptest`) in `src-tauri/tests/chunking_test.rs` verify the
    word-count bound (excluding atomic Table/Figure) + contiguous `chunk_index` for any
    input.
  - **`src-tauri/src/db/migrations/v002_wiki_manifest.rs`** - Post-v001 schema
    (VERSION 2, deployed). Contains only `CREATE TABLE wiki_index_manifest` (per-file
    content hashes for the Wiki external-edit drift detection). The FTS5 drop,
    `article_chunks` creation, and `audit_entries` rebuild were in v002 pre-release but
    moved to v003 after v002 was deployed with only `wiki_index_manifest`. v001 is
    updated so fresh DBs get the expanded audit CHECK constraint directly.
  - **`src-tauri/src/db/migration.rs`** - migration runner. **Transactional**:
    each migration's `up_sql` + `user_version` bump run in a single
    `unchecked_transaction` so a crash between the DDL and the version pragma
    rolls back cleanly (previously they were two autocommit statements, so a
    force-quit between them left the DB half-migrated). **Self-healing
    pre-pass** (`heal_partial_migrations`): detects DBs corrupted by older
    non-transactional builds by probing for the v003 marker column
    (`articles.is_translated`) while `user_version < 3`; if present it advances
    `user_version` to 3 without re-running the dangerous `ALTER TABLE ADD
    COLUMN` statements (SQLite has no `IF NOT EXISTS` for ADD COLUMN). Future
    migrations that add another `ALTER TABLE ... ADD COLUMN` MUST extend
    `heal_partial_migrations` with a marker-column check. 5 inline unit tests
    + `tests/migration_recovery_test.rs` (3 integration tests simulating the
    partial v003 state that crashed pre-fix builds).
  - **`src-tauri/src/db/migrations/v003_articles_translations.rs`** - Post-v002 schema
    (VERSION 3). Carries the reverted v002 content (FTS5 drop, `article_chunks`,
    `audit_entries` rebuild with `figure_descriptions` + `ai_screen_enhanced`) plus
    translation schema: `articles` columns (`is_translated`, `translation_status`,
    `translation_error`, `translated_at`), `article_original_content` +
    `article_original_chunks` tables, and `audit_entries` CHECK expansion for
    `translation` + `translation_error`. The `ALTER TABLE ADD COLUMN` statements
    have no `IF NOT EXISTS` guard (SQLite limitation), so the transactional
    runner + `heal_partial_migrations` pre-pass in `db/migration.rs` are the
    contract that prevents duplicate-column crashes on re-run. Plan:
    `.worktrees/language-plan-v2.md`.
  - **`src-tauri/src/db/migrations/v005_audit_note_add.rs`** - Post-v004 schema
    (VERSION 5, **not yet deployed**). Two additions folded into one migration:
    (1) rebuilds `audit_entries` to add `'note_add'` to the `action` CHECK
    constraint (the `update_article_notes` command previously reused
    `'status_change'`, making note edits appear as status changes; same
    rename-create-copy-drop pattern as v003/v004); (2) creates
    `idx_articles_translation_status` on
    `articles(translation_status, is_translated)` so the startup
    stranded-recovery query is an index range scan instead of a full table
    scan - folded directly into v005 (rather than a separate v006) because
    v005 had not been deployed yet. Both operations are idempotent, so
    `heal_partial_migrations` is not needed. v001 is updated so fresh DBs get
    `note_add` in the initial CHECK constraint directly.
    Frontend `AuditAction` type + `formatAuditAction` labels +
    `audit-timeline.vue` `actionLabels` all include `note_add` -> "Note Added".
    The dashboard Recent Activity feed now carries `articleId` on each entry
    so the dot icon is a clickable button (title "Go to article") that
    deep-links to `/articles?articleId=<id>`, opening the article detail in
    the All articles view. The activity layout is compacted (24px dot, 13px
    text, tighter padding) and all action labels use Title Case.
    **Audit entry coalescing** (`audit_repo::create_or_update_entry`):
    when a second audit entry with the same `article_id + action + source`
    arrives within `COALESCE_WINDOW_SECS` (300 seconds / 5 minutes), the
    existing row is **updated** (details + timestamp) instead of inserting
    a new row. This prevents audit-trail spam when the user makes several
    rapid edits of the same type (e.g. adding 3 labels one at a time
    produces a single `label_add` entry showing the final count). Used by
    `update_article_notes`, `update_article_tags`, `update_article_labels`,
    and `update_article_criteria`. Entries with different actions, articles,
    sources, or timestamps older than the window are NOT coalesced. Tested
     in `tests/audit_coalesce_test.rs` (5 tests: coalesce rapid same-type,
     different actions, different articles, different sources, expired
     window).
  - **`src-tauri/src/db/migrations/v006_audit_metadata_edit.rs`** - Post-v005 schema
    (VERSION 6). Extends the `audit_entries.action` CHECK constraint to include
    `'metadata_edit'` so in-place metadata field edits (Authors, Affiliation,
    Journal, Year, Lang, DOI, Keywords) in the Article Detail "Metadata" card
    are correctly categorized. Same rename-create-copy-drop pattern as
    v003/v004/v005; idempotent so `heal_partial_migrations` is not needed. v001
    is updated so fresh DBs get `metadata_edit` in the initial CHECK constraint.
    The `update_article_metadata` Tauri command (in `commands/articles.rs`)
    writes audit rows with `action = 'metadata_edit'` and `details =
    "Metadata edited: <Field>"` (e.g. "Metadata edited: DOI"), coalesced within
    the 5-min window so rapid multi-field edits produce a single audit row.
    Calls `mark_biblio_needs_refresh` + `mark_wiki_needs_refresh` since metadata
    changes (authors, journal, year, keywords) feed both pipelines. Backend
    whitelist: `ArticleMetaField` enum (`article_repo.rs`) validates the column
    name (no string interpolation); `ArticleMetaValue` is `#[serde(untagged)]`
    scalar-or-array (arrays for Authors/Keywords, scalars for the rest). Empty
    strings clear to NULL; `publication_year` parses to `Option<i32>`.
    Frontend: `article-metadata.vue` always renders all fields (empty → muted
    `---` placeholder) and double-clicks any field to edit in place
    (`nextTick` focus+select, `Enter`/blur commits, `Escape` cancels — same
    pattern as `tag-label-panel.vue` v6.9). Field-specific validation: **Year**
    requires a 4-digit integer in `[1800, 2100]` (frontend blocks invalid
    commits with a red hint; backend range guard clears out-of-range to NULL as
    defense-in-depth); **Journal** re-resolves `journal_index_id` on every edit
    via `journal_repo::resolve_journal_id` so the bibliometric pipelines stay
    in sync — when the typed name is not in the local index, `journalIndexId`
    is `null` and the label shows an amber "(unrecognized)" annotation (the
    entry is still accepted; "Rematch Journals" in Settings retries); **Lang**
    is a `<select>` dropdown of ~24 common academic languages with an "Other…"
    option that reveals a free-text input for custom values. Tested in
    `tests/article_metadata_test.rs` (19 Rust tests: 7 field round-trips +
    year range guard boundaries + journal re-link recognized/unrecognized +
    empty/whitespace clear + audit row) +
    `src/__tests__/components/article-metadata.test.ts` (15 inline-edit +
    placeholder + validation tests).
  - **`src-tauri/src/wiki/fts.rs`** (T1.2 update) - chunk-aware FTS5 schema:
    `ensure_table` now creates `chunk_index UNINDEXED, section UNINDEXED, parent_slug
    UNINDEXED` columns. `PageRow` carries `chunk_index: Option<i32>`, `section:
    Option<String>`, `parent_slug: Option<String>`. `WikiPageHit` surfaces the same three
    fields. `ensure_index_populated` self-heal compares `COUNT(DISTINCT COALESCE(
    parent_slug, slug))` against disk page count (not raw row count) so chunk rows do
    not false-positive a rebuild on every chat call. `search` SELECT + row mapping
    updated for the 3 new columns. All 36 unit tests for this module live in
    `tests/wiki_fts_test.rs` (extracted from the inline `#[cfg(test)]` block per
    `docs/CLAUDE.md` §Testing; the source file shrank from 1482 to 680 lines).
    `strip_table_placeholders` is `pub` so the integration test can exercise it
    directly.
  - **`src-tauri/src/wiki/chat.rs`** (T1.2 update) - chunk-aware context builder:
    `MAX_HITS` raised from 8 to 16. `build_context` dedupes by `parent_slug` (keeps
    top-ranked chunk per page, appends "(+N more passages from this page)"). `format_entry`
    includes `(§Methods)` in the header when `hit.section` is present so the model can cite
    the passage. 3 new tests: section-label-in-header, dedupe-chunks-of-same-page,
    distinct-pages-not-deduped.
  - **`src-tauri/src/batch_import/`** - 3-phase batch import processor. Scans the
    Bango Documents directory for files produced by external tools and imports
    them into the article database by DOI match. Files keyed on
    `clean_doi_filename(normalized_doi)`, consistent with Citation Chaser RIS
    naming (`{clean_doi}_references.ris`). Modules: `mod.rs`
    (`BatchImportRunner`, `BatchImportProgress`, `BatchImportState` managed
    state, `start_batch_import` / `cancel_batch_import` /
    `get_batch_import_progress` Tauri commands; spawned `tokio::task` so the UI
    stays responsive and the user can navigate away; cancel token checked
    between items; emits `batch-import:progress` events), `full_text_phase.rs`
    (Phase 1: scan `fulltext/` for `{cleaned_doi}.pdf` / `.txt`, attach via
    extracted `commands::full_text::attach_full_text_inner`; skips articles
    with `has_full_text=true`; returns newly-attached IDs for Phase 3;
    text-extraction failures are handled inside `attach_full_text_inner` as
    soft-fallback attaches with empty `full_text` + a `log_error` audit row,
    and hard attach failures (missing file, copy error, DB write error) write
    a `log_error` audit entry in addition to the in-memory progress errors),
    `citations_phase.rs` (Phase 2: scan `ris/` for
    `{cleaned_doi}_references.ris`, `_citations.ris`, `.ris`, `.bib`; skips
    articles with `has_reference_details`/`has_citation_details`; auto-detects
    RIS vs BibTeX by extension via extracted
    `commands::references::import_references_inner`), `translations_phase.rs`
    (Phase 3: enqueue `FullText` translation jobs for non-English
    newly-attached articles via `enqueue_article_translation_inner` + poll
    until each completes; runs only when `auto_translate=true`; **pre-flight
    LLM-configured guard** (`check_llm_configured_or_skip`, pure `&Connection`
    helper) short-circuits the phase with the canonical
    `"Skipped: LLM not configured"` message + a system-level audit record via
    `audit_repo::log_error` so the skip surfaces in Diagnostics / Notification
    History instead of churning every article through the worker's per-article
    failure path - mirrors the Phase 4 pre-flight pattern), `summary_phase.rs`
    (Phase 4: generate AI summaries for newly-attached articles without an
    existing summary; reuses extracted
    `commands::summary::generate_article_ai_summary_inner` so behavior is
    identical to the article detail "Generate AI Summary" button including the
    `include_section_summaries` flag; **pre-flight LLM-configured guard**
    (`llm_configured_with_audit`) short-circuits the phase with the same
    `"Skipped: LLM not configured"` message + system-level audit record).
    `db::article_repo::get_articles_with_doi_info` loads all articles with a
    non-null DOI + the `has_full_text` / `has_reference_details` /
    `has_citation_details` / `has_ai_summary` flags in a single query to build
    the DOI match map. Frontend: `settings-reprocessing.vue` (button "Import
    full text files" right after "Rebuild text chunks"; dialog explaining the
    4 phases + file naming convention + Start/Cancel; live progress bar with
    phase label + per-phase completed/total + overall percent + cancel button;
    per-phase summary lines surface skip messages - e.g.
    "Skipped: LLM not configured" - with a warning style via
    `phaseSkipMessage(phase)` so the user understands why a phase did nothing;
    listens to `batch-import:progress` events so it survives navigation). 8
    inline tests in `full_text_phase.rs` (DOI match map normalization +
    collision + empty skip) + `citations_phase.rs` (skip-when-has-details,
    find-references, find-citations-independently, generic-ris-fallback,
    generic-bib-fallback). End-to-end integration tests live in
    `tests/batch_import_test.rs` (12 tests: Phase 1 attach + skip-already-attached +
    no-matching-DOI + no-DOI-article; Phase 2 refs + citations + independent +
    skip-already-has-details; full-pipeline idempotency; multiple articles with
    mixed files; Phase 3 pre-flight skip + audit + proceed). Phase 3 (live
    translation) + Phase 4 (AI summaries) require a live LLM and are not covered
    end-to-end; the pre-flight LLM gate is unit-tested via the pure
    `check_llm_configured_or_skip` helper, and the `generate_article_ai_summary_inner`
    core is tested via the existing `summary_engine_test.rs` mock-LLM path.
- **`src/`** - Vue 3 + TypeScript + Tailwind v4 frontend.
  - **`src/assets/demo-project.bango.json`** - bundled demo project (loaded as raw text
    via `?raw` by `src/composables/use-demo.ts` and passed to `import_project_backup`).
    Contains 25 articles (11 included, 1 rejected, 2 duplicate, 11 working) spanning
    2015-2025, with populated `articleTags`/`articleLabels` junction tables. The two key
    UK SDIL papers (Gressier 2025, Dickson 2025) plus 7 additional real UK sugar-levy
    studies (Cobiac, Rogers, Pell, Bandy, Amies-Cull, Gillieson) form the included corpus
    that powers all six bibliometric tools. Only two articles carry AI analysis metadata
    as examples (seq 3 rejected via geography exclusion, seq 14 included via substance
    scope). `referencePapers`/`articleReferenceLinks` are left empty for the user to
    populate via reference/citation imports. `scripts/enrich_demo.py` is the idempotent
    generator (deterministic UUID5 article IDs); re-run after schema changes.
  - **`src/views/`** - page-level views. `article-list.vue` (`/articles`) and
    `wiki-view.vue` (`/wiki`) are **keep-alive cached** via
    `<keep-alive :include="['WikiView', 'ArticleList']">` in `app-shell.vue` so their
    UI state survives navigation away and back. Both components name themselves via
    `defineOptions({ name: ... })` (required for the `include` matcher to find
    `<script setup>` components). `article-list.vue` caches: active status tab,
    applied filters (panel + query), sort column/direction, current page + page size,
    toolbar search text, multi-select set, opened article detail panel + audit trail,
    and fullscreen state. Its `onActivated` (skipped on the first activation via an
    `isFirstActivation` guard) refreshes the underlying data so the view reflects
    changes that happened while away: `search()` re-runs the preserved `query`
    (rows + tab badges update), and the open article detail + audit trail are
    re-fetched. Route deep-link params (`?articleId=…`, `?status=…&tags=…`,
    biblio/tag/label deep-links) override the preserved state when they differ
    (explicit navigation wins). The References and Search tabs skip `search()`
    (their child components own their data) but still refresh tab badges. The
    other three `useArticleSearch()` consumers (`wiki-view.vue`, `chat-view.vue`,
    `biblio-citations.vue`) are NOT affected - they keep creating fresh per-view
    composable instances as today. `biblio-dashboard.vue` is the `/bibliometrics`
    parent; child routes (`coauthors`, `citations`, `keywords`, `timeline`, `authors`)
    render in its `<router-view>`. `biblio-timeline.vue` is the Publication Timeline view
    (its secondary "Top Journals" chart auto-hides below `SECONDARY_CHART_MIN_VIEWPORT_HEIGHT`
    = 700px viewport height, driven by the reactive `height` ref from `use-viewport.ts`);
    `biblio-authors.vue` is the Author Productivity Ranking view (sortable table + slide-over
    detail panel + Google Scholar external lookup icons). `help-guide.vue` is the `/help` shell
    (tab bar + `?tab=`/`#hash` deep-link routing) that renders one `help-tab-*.vue` component
    per tab (guide, bibliometrics, troubleshooting, local-ai, reference); the Bibliometrics tab
    documents all six completed modules. `help-tab-reference.vue` is the sidebar +
    scroll-spy Reference tab; the Wiki section (`ref-wiki`, nav icon `local_library`) sits
    under Chat Assistant and covers the wiki-root layout, getting-started workflow, supported
    document file-type matrix, FTS5 token-optimized chat, and Obsidian integration.
    `wiki-view.vue` is the `/wiki` route (flat, below
    `chat-view.vue` is the `/chat` route. It renders the article-RAG chat (explicit
    selected articles via `send_chat_message`) AND the wiki-RAG chat: a Wiki toggle button
    (icon `local_library`) sits right of the `(+)` icon, visible only when
    `chatStore.wikiReady` (wiki initialized AND `pageCount > 0`, populated from
    `wiki_get_status`). When active it gets an indigo halo/fill, hides the `(+)` button
    + article context pills, shows a "Wiki mode" banner, and routes sends through
    `wiki_chat` (BM25 FTS5 RAG) instead of `send_chat_message`. Each message records its
    `source` (`'articles'|'wiki'`) so the bubble shows a `wiki` badge and the assistant
    body is rendered via `src/utils/wiki-markdown.ts` with `articlePriority: true` plus a
    reactively-derived `wikiSources` map (article id -> WikiSourceInfo, built from the
    loaded `articles` list) and the `wikiPageTitles` map, so bare article UUIDs in wiki
    prose render as green `.art-ref` chips (article detail) while wiki-page UUIDs render
    as pink `.wikilink--synthesis` chips (wiki reader). `[^art-id]` becomes `.art-ref`. Clicking a
    wikilink opens a right-side **Wiki reader slide-over** (`WikiPageViewer` with a
    `wikiNavStack` back-stack so inner navigation chains and a Back/Close chrome returns
    to the chat); opening it closes the article detail slide-over and vice-versa (mutually
    exclusive). `wiki-view.vue` is the `/wiki` route (flat, below
    `/chat` in `nav-sidebar.vue` with the `local_library` icon). Ships the empty-state gates
    (LLM configured, included articles > 0, wiki initialized), the sidebar (page list
    grouped by type + client-side search filter), the page viewer (`wiki-page-viewer.vue`
    with `[[wikilink]]` + `[^art-id]` source ref resolution + article detail slide-over),
    the split-pane editor (`wiki-page-editor.vue`), the sigma graph view
    (`wiki-graph-panel.vue` with ForceAtlas2 layout, color-coded by page type), and the
    toolbar (`wiki-toolbar.vue`: Re-scaffold one-click pipeline, Add Documents, Lint,
    Delete Wiki, progress bar, plus a single-purpose Chat button that deep-links into
    `/chat` with Wiki mode pre-enabled). Composable: `use-wiki.ts`; types: `types/wiki.ts`. The
    page action bar carries **Back/Forward** navigation icons (left of Edit) backed
    by the generic `useNavHistory<string>` composable (see `src/composables/`), plus
    platform-aware keyboard shortcuts registered via `window.addEventListener('keydown', ...)`
    in `onMounted` / removed in `onUnmounted`: macOS `Cmd+[` / `Cmd+]` (and `Cmd+Left` /
    `Cmd+Right`); Windows/Linux `Alt+Left` / `Alt+Right`. Shortcuts are disabled while focus
    is in an input/textarea/contenteditable, in edit mode, on the Graph tab, or at the
    history bounds. `selectedSlug` is a read-only computed alias over the history's current
    entry; all mutations go through `navigate()` / `goBack()` / `goForward()` / `clear()`.
  - **`src/components/openalex-search.vue`** - OpenAlex Search tab content: search bar,
    Smart Search button, sort + pagination controls, results list with sticky action bar
    (Select All + Add to Working + Clear), and split-window detail panel. Renders
    `<OpenAlexResultItem>` and `<OpenAlexDetailPanel>`.
  - **`src/components/openalex-result-item.vue`** - Single search result row: checkbox,
    title, meta line (author, journal, year, OA badge, cited-by count), 200-char snippet,
    "Already in library" grey-out, "Retracted" badge, indigo border + box-shadow halo
    when its detail panel is open.
  - **`src/components/openalex-detail-panel.vue`** - Right-side split-window detail panel:
    full abstract, authors with affiliations, journal/biblio metadata, DOI link, keywords,
    OA/PDF links, "Add" button (single-article import), "Close" button, "Open in OpenAlex"
    external link.
  - **`src/components/settings/settings-openalex.vue`** - OpenAlex settings card: API key
    (encrypted via AES-256-GCM, clear/replace), mailto email (polite pool), Retrieve
    References toggle (default off, with rate-limit warning).
  - **`src/stores/openalex.ts`** - Pinia store for session-scoped search state (query,
    results, pagination, sort, filters, selection, loading/error, smartSearchAvailable).
    Search, import (`importSelected` + `importSingle`), and DOI-library-check actions
    wrap the Tauri commands. Tested by `src/__tests__/openalex-store.test.ts` (5 tests).
  - **`src/types/openalex.ts`** - TypeScript interfaces for OpenAlex API types +
    `SORT_OPTIONS`, `PER_PAGE_OPTIONS`, `DEFAULT_OPENALEX_FILTERS` constants.
  - **`src/components/`** - reusable components. `journal-info-card.vue` lazily loads
    journal metadata via the `biblio_get_journal_info` command. `help/` holds the five
    `help-tab-*.vue` tab components consumed by `help-guide.vue`; shared card styles live in
    `src/styles/help-shared.css`. `settings/` holds the settings sub-components consumed by
    `settings-view.vue`: `settings-provider-card.vue` (consolidated AI Provider box - warning +
    connection details + parameters + Revert/Get Models/Test Connection + test-result/error
    feedback in one bordered `<section>`), `settings-ai-summaries.vue` (3 toggles:
    auto-generate-summaries [localStorage key `bango-full-text-summaries`:
    auto-fire whole-paper summary on attach], section-summaries [localStorage key
    `bango-section-summaries`; manual `auto_awesome` button always works regardless],
    auto-translate [DB-backed `app_settings.auto_translate`; experimental; default
    enabled; translates non-English articles to English during AI processing - see
    `app_settings_repo.rs` entry above]),
    `settings-screening-preferences.vue` (screening-mode dropdown + auto-navigate toggle;
    all three modes always selectable, with a fallback/active notice driven by
    `get_full_text_article_count`),
    `settings-storage.vue` (storage root picker + directory-tree visual),
    `settings-reprocessing.vue` (text-chunks rebuild + Batch Import processor:
    full-text attach + Citation Chaser RIS import + optional AI summary
    generation, 3-phase pipeline with progress bar and cancel; see
    `batch_import/` entry below),
    `settings-project-management.vue` (import/export/delete
    + dialogs; Delete All Data also wipes the on-disk Wiki and resets
    `useWiki`/`useChatStore.wikiReady`; Export dialog warns that the Bango Documents
    directory - full-text PDFs + Wiki - is NOT backed up),
    `settings-notification-history.vue` (in-memory toast history viewer; newest-first list with
    type-colored dots, timestamps, and a Clear History button; reads the `history` ref from
    `use-toast`; sits immediately before Diagnostics in the card stack), and
    `settings-diagnostics.vue` (error log). Shared card chrome for these lives in
    `settings-card-shared.css`.
  - **`src/composables/`** - Vue composables. `use-startup-upgrade.ts`
    (silent legacy DB upgrade orchestration: `getStartupStatus` calls the backend
    `get_startup_status`, `performLegacyUpgrade` calls `perform_legacy_upgrade`;
    `decideUpgrade(needsUpgrade, alreadyAttempted)` is the pure loop-guard decision
    returning `'run'` | `'skip'` | `'stale'`, backed by a session-scoped
    `sessionStorage` flag via `getUpgradeAttempted`/`markUpgradeAttempted`;
    consumed by `main.ts` `bootstrap()`; tested by
    `src/__tests__/composables/use-startup-upgrade.test.ts`),
    `use-bibliometrics.ts` (shared KPI
    singleton; on mount fetches KPIs then the
    `biblio_get_needs_refresh` flag and auto-runs `runNormalization` when
    `includedCount > 0 && needsRefresh` - this starts the Refresh cycle on dashboard
    entry and the backend clears the flag after `biblio_normalize` commits;
    `runNormalization` also drives the 8-step `biblio:progress` bar), `use-journal-info.ts`
    (per-call lazy loader), `use-article-search.ts` (supports
    `yearFrom`/`yearTo`/`journal` route params), `use-network-view.ts` (shared
    view-state composable consumed by the four bibliometric network views
    `biblio-coauthors`/`biblio-keywords`/`biblio-cocitations`/`biblio-citations`;
    owns cross-cutting state - focus, visible counts, color/layout modes, cluster
    selection, sidebar collapse - plus the identical handlers: cluster toggle,
    layout-mode switch, PNG/GEXF export via `exportPrefix`, and subgraph
    recalculate that respects `graphType: 'directed'|'undirected'` and
    `yearAttribute: 'year'|'avgYear'`). Tested by `src/__tests__/use-network-view.test.ts`.
    `use-nav-history.ts` (generic `<T>` browser-like navigation history: `navigate`
    pushes + truncates forward history + skips duplicates; `goBack`/`goForward` no-op at
    bounds; `clear` wipes the stack. Consumed by `wiki-view.vue` for Back/Forward page
    navigation. Pure logic, no DOM/Tauri deps; tested by
    `src/__tests__/composables/use-nav-history.test.ts`.
    `use-full-text-attachment.ts` (shared UI orchestration for attaching a full-text
    PDF/TXT via the OS file dialog: open dialog -> "Importing..." toast -> caller's
    `attachFullText` (sourced from `useArticleSearch`) -> "success" toast -> optional
    `onAttached` hook. Centralizes the flow that was previously duplicated across the
    four `ArticleDetailPanel` host views - `article-list.vue` (with auto-summarize hook),
    `biblio-citations.vue`, `chat-view.vue`, `wiki-view.vue`. Error toasts include the
    underlying message so all four views report failures with equal detail. The
    low-level IPC + refresh logic stays in `useArticleFullText`/`useArticleSearch`; this
    composable owns only the file-dialog + toast shell).
    `use-gap-analysis.ts` (Research Gap Analysis singleton: `gapText`/`loading`/`error`/
    `generatedAt` refs + `generate(style)`/`loadSaved`/`clearGapAnalysis`/`formatGeneratedAt`;
    mirrors `use-summary.ts` 1:1 and backs the "Research Gap Report" button in
    `summary-view.vue`; calls `analyze_research_gaps` + `get_saved_gap_analysis`).
  - **`src/utils/`** - pure utilities: `network-export.ts` (graph PNG/GEXF export via the
    `save()` + `write_text_to_file` pattern), `formatters.ts`, `color.ts`, `debounce.ts`,
    `next-paint.ts`, `reference-flatten.ts`, `citation-analysis.ts`, `graph-filters.ts`
    (pure graph-visibility filters extracted from `use-sigma-renderer.ts`:
    `applyGraphFilters`/`applyCitationGraphFilters`/`applyKeywordGraphFilters` mutate
    `hidden` attributes on a `graphology` instance + return visible-node/edge counts;
    unit-tested in `src/__tests__/utils/graph-filters.test.ts` without DOM/Sigma scaffolding),
    `llm-error.ts`,
    `google-trends.ts` (Trends embed URL builder + date-range validators),
    `wiki-markdown.ts` (shared wiki Markdown renderer: `renderWikiMarkdown(text, opts?)`
    converts `[[slug]]` / `[[slug|alias]]` to `.wikilink` anchors and `[^art-id]`
    footnotes to `.art-ref` anchors (with `data-slug` / `data-art-id` attrs); a
    `staticMode` post-pass (`convertVueLinksToStatic`) rewrites those anchors to
    standard `<a href>` for the static-site exporter (missing targets render as
    `<span class="ref-missing">`). T2.3
    Phase 3 added `(§Section)` suffix parsing: a citation like `[[slug]] (§Methods)`
    renders the chip plus a muted `<span class="section-badge">§Methods</span>` badge
    so the reader can locate the passage. The badge styling lives in
    `wiki-page-viewer.vue` and `chat-view.vue` (scoped `:deep(.section-badge)`).
    On author pages the viewer passes `linkArtRefsToSynthesis: true` so each
    publication's `[^art-{uuid}]` opens the wiki synthesis page (slug = uuid,
    pink `.wikilink--synthesis` chip) instead of the article detail; the flag
    falls back to a green `.art-ref` when no synthesis page exists for the uuid.
    The renderer HTML-escapes slug/alias text, strips `/raw/*.md` artifact lines
    (including title-based paths with spaces), collapses dangling non-UUID
    footnote refs (so `[^title]` / `[^1]` markers don't leak as literal text
    but `[^uuid]` is resolved, not stripped), then runs
    `marked.parse`. Bare UUIDs in prose are auto-linked: `articlePriority: true`
    (chat view) resolves `sources` first -> green `.art-ref` (article detail);
    otherwise `pageTitles` wins -> pink `.wikilink--synthesis` (wiki reader).
    Article-matched UUIDs always emit `.art-ref` (green, article detail) instead of
    the former `[[uuid|alias]]` (which became an indigo wiki link). Consumed by both
    `wiki-page-viewer.vue` (sources + pageTitles, default priority) and `chat-view.vue`
    assistant bubbles (sources + pageTitles + `articlePriority: true`). Pure function,
    unit-tested in `src/__tests__/utils/wiki-markdown.test.ts`).
    **External-document citation routing** (regression fix for
    `[^art-user-youcantbuild]` mangled-HTML bug): the footnote regexes accept any
    kebab/snake slug (`[a-z0-9_-]+`), not just hex UUID chars, so refs to uploaded
    documents resolve. Smart click routing: non-UUID ids with a `pageTitles` entry
    (the Layer-1 source page) route to a pink `.wikilink--synthesis` chip opening the
    wiki source page; UUID ids with `sources` stay green `.art-ref` (article detail).
    `raw_export.rs::resolve_user_file_title` enriches PDF titles via `lopdf` (reads
    the `/Title` entry from the Info dictionary) so the pre-seeded source page + the
    LLM prompt use the document's real title instead of the filename stem.
    `wiki-site-export.ts` (static-site export engine: gathers pages + sources via
    existing wiki commands, renders each page to standalone HTML reusing
    `renderWikiMarkdown(staticMode)` with depth-aware `slugToHref`/`artIdToHref`
    resolver closures, generates article-stub pages for referenced articles
    without synthesis pages, builds `index.html` + `style.css` + `search.js` +
    `search-index.json`, opens the save dialog, and passes a `SiteExportBundle`
    to the `wiki_export_site` backend command). Pure helpers tested in
    `src/__tests__/utils/wiki-site-export.test.ts`.
    `platform.ts` (`isMacPlatform()` reads
    `navigator.platform`; `SHORTCUT_MODIFIER` constant resolves to `'Cmd'` or `'Alt'`.
    Dependency-free, resilient to `navigator` absence. Used by `wiki-view.vue` to pick the
    correct back/forward keyboard shortcut modifier. Tested by
    `src/__tests__/utils/platform.test.ts`).
  - **`src/stores/chat.ts`** - Pinia chat store. Holds `selectedArticleIds`, `messages`,
    `loading`, `error`, plus the retrieval-source state `source: 'articles'|'wiki'`
    (default `'articles'`; mutually exclusive) and `wikiReady` (drives the chat-view wiki
    toggle visibility). `sendMessage(text)` branches: `source==='wiki'` calls
    `wiki_chat` (history-only payload, no articleIds); otherwise calls `send_chat_message`
    with `selectedArticleIds`. Each pushed message records its `source` for bubble
    rendering. `toggleWikiMode()` flips the source; `clearChat()` resets it to
    `'articles'`. Tested by `src/__tests__/chat.test.ts`.
  - **`src/styles/forms.css`** - global form/button/dialog primitives (`.field__*`, `.btn--*`,
    `.dialog`, `.dialog__danger-box`, `.dialog__info-box`, `.spinner`) promoted from the
    former scoped `llm-config.vue`. Loaded via `base.css`; low specificity so scoped rules
    in other views still win.
  - **`src/router/index.ts`** - route table; lazy views are prefetched after `router.isReady()`.
    `/settings` renders `settings-view.vue`.
- **`landingpage/`** - standalone marketing microsite (NOT part of the shipped Tauri
  app). Static HTML5 + Tailwind v4 (browser CDN build, no compile step). Two pages:
  `index.html` (hero, privacy callout, feature grid, how-it-works, final CTA, footer)
  and `help.html` (static reference copy of the in-app 5-tab Help system: User Guide,
  Bibliometrics, Troubleshooting, Local AI, Reference). Shared `assets/` (logo.png +
  screenshots). Cross-linked: `index.html` nav carries a `Help` link to `help.html`;
  `help.html` nav links back to `index.html` sections. Both pages ship the same
  vanilla-JS primitives: LinkedIn Insight Tag + conversion tracking
  (`window.lintrk('track', ...)`, conversion_id 28476826), scroll-reveal via
  IntersectionObserver, and an image lightbox (click-to-enlarge + Esc/backdrop/(X)
  close). `help.html` additionally loads the Material Symbols font for icons and a
  tab-switching IIFE that mirrors the app's 5-tab shell. Destination for all CTAs:
  Microsoft Store (`apps.microsoft.com/detail/9np2bhgxt8h3`). Live home:
  https://bango.boncode.net. When porting app Help content to `help.html`, remove
  app-only interactivity (Vue router navigation buttons, demo-project loader,
  scroll-spy sidebar) and replace CSS variables / Tailwind-scoped styles with plain
  CSS or self-contained utility classes.
- **`tests/test-citations/`** - RIS fixture data for citation/reference system tests.
  `main_articles.ris` (10 articles, DOIs `10.1001/art1`–`10.1010/art10`) with per-article
  `_references.ris` and `_citations.ris` files (filename = DOI with `/`→`_`). A dedicated
  co-citation dataset uses `co-citation.ris` (5 articles, `10.2001/cocite1`–`10.2001/cocite5`)
  with 6 shared reference papers (`10.3001/ref1`–`10.3001/ref6`) spread across the
  `_references.ris` files to produce deterministic co-citation pairs.
- **`docs/bango-v4-spec.md`** - authoritative v4 product specification.
- **`docs/CLAUDE.md`** - project coding rules (Rust/TS error handling, naming, LLM
  orchestrator pattern, DB rules, testing conventions).
- **`docs/test-coverage-report.md`** - coverage baseline + under-coverage analysis for
  Rust (`cargo-llvm-cov`, ~52% lines) and Vue/TS (`@vitest/coverage-v8`, ~18% lines).
  Lists 0%-covered modules/components/composables/stores and ranks highest-value gaps.
- **`docs/design-reference/00-design-patterns.md`** - design tokens (Material 3 inspired).
- **`docs/test-plans/`** - binding test inventory files consumed by
  `scripts/check-test-inventory.sh` (wired into `npm run check:all`). Each plan that
  specifies a Test Inventory section places its machine-checked `file::function` table
  here as `<plan-name>-tests.md` so the script can grep-named test files at PR time.
  Current: `language-plan-v2-tests.md` (26 rows across 11 files),
  `translation-3-tests.md`, `search-strategy-tests.md` (8 rows: Search
  Strategy Builder pure helpers), `wiki-export-tests.md` (12 rows: Wiki
  static-site export zip + markdown-tree + staticMode helpers).
- **`.worktrees/`** - planning documents (`language-plan-v2.md` is the active
  translation plan; the superseded `language-plan.md` is archived in `DONOTUSE/`;
  implemented/temporary docs are archived in `DONOTUSE/`, such as the timeline plan
  `biblio-publication-timeline-plan-v3.md` and the Search Strategy Builder plan
  `improve-search-final.md`). Not part of the shipped app.

Verification gate: `npm run check:all` (type-check + eslint + prettier + rustfmt + clippy
`-D warnings` on the library crate + vitest + `check:test-inventory`) and
`cargo test`. The clippy rule lives in `src-tauri/Cargo.toml`
`[lints.clippy]` (escalated to deny by `-D warnings`); `unwrap_used`,
`expect_used`, and `panic` are re-asserted test-aware in `src-tauri/src/lib.rs`
via `#![cfg_attr(not(test), warn(...))]` so they fire on production code but
not on test code (see `docs/CLAUDE.md` §Error Handling).

Coverage tooling: `npm run test:coverage` (Vue/TS via `@vitest/coverage-v8`, config in
`vitest.config.ts`, report at `coverage/index.html`) and
`cd src-tauri && cargo llvm-cov --html --output-dir target/llvm-cov/html` (Rust via
`cargo-llvm-cov` + `llvm-tools-preview`, report at
`src-tauri/target/llvm-cov/html/html/index.html`). Both artifact dirs are git-ignored.
