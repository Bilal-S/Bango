# screening/

## Purpose

Tier 3 AI screening engine with three modes (Abstract / Enhanced / Two-stage).
Evaluates articles against inclusion/exclusion criteria, optionally using
full-text chunk evidence, and writes decisions + audit entries.

## Ownership

- Owns: `engine/` (directory module: `mod.rs` + `types.rs` + `prompt_parts.rs`
  + `stage2.rs`), `decision.rs`, `error_classify.rs`, `json_parse.rs`,
  `tags_labels.rs`, `article_writer.rs`, `evidence.rs`, `resolution.rs`,
  `chunk_retrieval.rs`, `token_estimation.rs`, `prompt.rs`, `llm_client.rs`,
  `mod.rs`.
- Commands live in `commands/screening.rs`.
- Consumes: `llm::LlmOrchestrator` (via `send_with_type`), `db::article_repo`,
  `db::chunk_repo`, `db::app_settings_repo`, `utils::text_tokens`.

## Local Contracts

### `chunk_retrieval.rs` - Tier 3 criteria-targeted chunk retrieval

Pure, `#[must_use]`. `rank_chunks_by_criteria(chunks, inc, exc, top_k,
max_chunk_words, budget)` scores chunks by criteria-token TF density (shared
`utils::text_tokens` tokenizer), boosts Methods-section matches, filters
oversized chunks, and enforces a per-article word budget. Also owns the
canonical `format_chunks_as_evidence(chunks) -> Option<String>` (the
`## Supporting Evidence` body formatter); `engine::format_chunks_as_evidence`
and `evidence::resolve_evidence` both delegate here so the chunks-only output
stays byte-identical across modes. Constants: `DEFAULT_TOP_K=2`,
`DEFAULT_MAX_CHUNK_WORDS=600`, `METHODS_BOOST=0.25`,
`DEFAULT_CHUNK_BUDGET_PER_ARTICLE=2400`. 11 inline tests + the §T3.7 inventory.

### Engine decomposition (refactor v3, see `.worktrees/refactor3.md` + `docs/test-plans/refactor3-tests.md`)

`engine` is a **directory module** (`engine/mod.rs` +
`engine/{types,prompt_parts,stage2}.rs`) holding the `ScreeningEngine` struct +
`run_sync`; the 285-line stage-2 borderline loop lives in `engine/stage2.rs`
(`run_stage2_borderline(&Stage2Context)`, returns `Ok(true)` on cancel /
`Ok(false)` on completion, `tokio::select!` cancel wrappers inline -
timing-sensitive contract). Types (`ScreeningConfig`, `RunSyncContext`,
`ScreeningProgress`, `LlmScreeningResponse`) live in `engine/types.rs`; shared
prompt construction (`ScreeningPromptParts` + `Stage2Context`) lives in
`engine/prompt_parts.rs`. Each submodule carries dedicated inline unit tests
(types serde, prompt-parts cloning, `is_borderline` predicate). Sibling modules
hold extracted free functions: `decision.rs` (`resolve_article_decision` pure
pipeline + `ArticleDecision`), `error_classify.rs` (`classify_llm_error` +
`LlmErrorOutcome`/`FatalReason` + leaf classifiers), `json_parse.rs`
(`process_screening_responses` + repair helpers), `tags_labels.rs` (sanitize +
create/match), `article_writer.rs` (DB writes + `mark_batch_screening_error`),
`evidence.rs` (Tier 4.1 complementarity + retrieval).

`ScreeningEngine::run_sync` takes a `RunSyncContext` struct (bundles
`request_delay_ms` + `app_handle` + `target_article_id` so the signature stays
under `clippy::too_many_arguments`). Both stages build prompts via
`ScreeningPromptParts::build_prompt_input` (single source of truth). Moved
symbols stay re-exported from `engine` (`pub use`) so external test import
paths keep working.

### Custom-logic governance contract (v8.1)

When `app_settings.screening_custom_logic` is present and non-empty (same trim
+ non-empty gate as the prompt's `## Custom Screening Instructions` emitter),
the LLM's `decision` is final and the generic §4.1 priority resolver
(`resolution::resolve_decision`) is NOT applied - the resolver cannot
understand combinatorial AND/OR/hard-exclusion rules, so it must not
second-guess the LLM. The engine computes `has_custom_logic` once per run and
routes both stage-1 and stage-2 decisions through
`resolution::finalize_decision(llm_decision, input, has_custom_logic)`, which
returns the LLM decision verbatim when custom logic is in force and otherwise
delegates to `resolve_decision` (drop-in `&str` return via lifetime). The
`[App override: ... favored due to priority resolution]` annotation is
naturally suppressed because `final_decision == llm_decision` whenever custom
logic governs. Projects without custom rules get byte-identical behavior to the
historical resolver.

### Single-attempt LLM call per batch (v8.1)

The screening engine makes one `send_with_type` call per batch. The previous
outer 429 retry loop (3 extra attempts, each bounded by the orchestrator's 600s
cap, no cancel-token check) was removed because (a) the inner
`client::send_with_retry` already handles transient 429/408/5xx with bounded
retry (3 attempts, exponential backoff 1s/2s/4s capped at 10s, honors
`Retry-After`), and (b) the outer loop multiplied the 600s cap by 4 (up to 40
min per batch) and ignored the cancel token, so Stop had no effect during the
retry sleeps - the user observed screening stuck at "30 of 50" for >10 minutes.
Any error from the orchestrator (including sustained 429) now marks the batch
as errors and moves on; the next batch benefits from `request_delay_ms` + the
orchestrator's concurrency semaphore. Sustained rate limits should be mitigated
by raising `request_delay_ms` in LLM settings, not by an outer loop that
ignores cancellation.

### Per-request-type timeout (v8.2)

The orchestrator's per-call wall-clock cap is now per-`LlmRequestType` via the
pure `#[must_use]` helper `llm::orchestrator::timeout_for(request_type) ->
Duration`. Screening (both stage-1 `Screening` and stage-2 `EnhancedScreening`)
uses `SCREENING_TIMEOUT_SECS = 120` (2 minutes); all other request types keep
the 10-minute default `LLM_TIMEOUT_SECS = 600`. Combined with the
single-attempt-per-batch contract above, a hung or slow screening call now
surfaces as an error within ~2 minutes instead of stalling the run.

### Immediate Stop + transient-error handling (v8.3)

Two coupled changes addressing the critique in `.worktrees/llmscreen.md`:

(a) Both stage-1 and stage-2 LLM calls are wrapped in `tokio::select!` against
a `tokio::sync::Notify` cancel signal. Clicking Stop drops the in-flight future
(cancelling the underlying `reqwest` request) within milliseconds instead of
waiting up to 2 minutes; the response is DROPPED (no DB write, no error
marking). The `cancel()` method calls `notify_waiters()` in addition to setting
the bool flag.

(b) Transient LLM errors (429, 401/403 Windows transient, 5xx, timeout,
transport) now leave articles UNSCREENED (no `screening_error`, no
`screened_at`) so the next run picks them up naturally - no manual "Reset
Errors" workaround. A new `after_sequence_id: Option<i64>` cursor on
`get_next_unscreened_working_batch` + per-run `last_attempted_seq` tracking
ensures the current run advances past failed batches instead of re-fetching
infinitely. The pure `#[must_use]` helper
`screening::engine::is_transient_llm_error(e) -> bool` classifies errors by
inspecting the message string (all LLM errors are `AppError::Import(String)`).
Non-transient errors (malformed JSON, parse mismatch) keep the existing
batch-error-marking behavior. The Windows 401/403 `insufficient permissions`
retry rationale is documented in the `is_retryable_response` doc-comment in
`client.rs` and in `src-tauri/src/llm/AGENTS.md`.

### Auto-stop + fixed phantom progress + actionable timeout (v8.4)

Three improvements addressing `.worktrees/llmscreen2.md`:

(a) Auth failures (401/403 without the Windows transient body) stop the run
immediately (threshold = 1) via new pure `#[must_use]` helper
`screening::engine::is_auth_failure(e) -> bool`. Other transients stop after
`TRANSIENT_FAILURE_THRESHOLD = 3` consecutive failures. Both set
`progress.fatal_error` so the frontend shows a red banner.

(b) Transient-deferred articles no longer inflate `progress.completed` or
`progress.errors` (the completion percentage was misleading). New
`progress.deferred` counter; the frontend renders a muted "N article(s)
deferred" notice.

(c) The orchestrator's timeout error now includes actionable guidance ("try
reducing batch_size or increasing request_delay_ms") instead of the opaque
"timed out."

### Cancellable inter-batch delay (v8.5)

All three `request_delay_ms` sleeps (success path, transient-error path,
stage-2 path) are now wrapped in the private `delay_or_cancel` helper, which
races the sleep against `cancel_notify` in a `biased tokio::select!`. Clicking
Stop now aborts the run within milliseconds during the inter-batch throttle
instead of waiting the full `request_delay_ms` (commonly 1-10s for rate-limit
mitigation). The helper MUST NOT use an `if *cancel_token` precondition on the
`notified()` select branch: tokio::select! skips polling a branch whose
precondition is false, which prevents `notified()` from registering as a
waiter, making `notify_waiters()` a no-op and silently losing the cancel signal
(the sleep runs to completion). Always poll `notified()` and check the token
inside the branch body. On cancel, the in-memory LLM response (if any) is
dropped - no partial DB writes, no error marking; articles stay unscreened for
the next run, matching the cancel-during-LLM-call contract. The v8.3 LLM-call
wrappers (stage-1 + stage-2) now ALSO use the always-poll-notified +
check-token-inside pattern (wrapped in a `loop` so the spurious-notify path can
`continue` without matching the LLM result type) - the earlier `if`
precondition was wrong: reqwest does NOT yield often enough during a slow LLM
response, so the cancel signal was lost and Stop had no effect mid-LLM-call.

### Slow-LLM warning + auto-stop (v8.6)

The `consecutive` transient-failure counter alone was insufficient because some
batches succeed between timeouts (resetting the counter), so a slow LLM limped
along without ever surfacing a user-visible message. Two new mechanisms address
this:

(a) `ScreeningProgress.warning: Option<String>` - a non-fatal yellow banner set
after the **1st** timeout ("The LLM is responding slowly… Consider reducing
batch_size"), cleared on the next successful batch; the frontend renders it as
`.screening-view__warning-banner` (amber, `warning` icon);

(b) `total_timeouts: u32` counter + `TOTAL_TIMEOUT_THRESHOLD = 3` - after 3
total (non-consecutive) timeouts, the run auto-stops with an actionable
`fatal_error`: "Screening stopped: the LLM timed out N times… Reduce
batch_size to 1-2 and restart. Already-screened articles are saved." This
catches the intermittent-timeout pattern where the consecutive counter resets
between failures.

(The v8.6 `batch_size` clamp reduction from `clamp(1, 15)` to `clamp(1, 5)`
was **reverted** - the clamp silently overrode the user's selection on the
unproven assumption that large batches cause hangs, which masked the real
per-batch behavior from the diagnostics. `commands/screening.rs` now honors
`1..=15` verbatim, matching the frontend stepper's `BATCH_MAX`; the
orchestrator timeout + auto-stop guards surface any genuinely slow provider
without baking in a batch-size assumption.)

### Diagnostics (v8.7)

Always-on screening instrumentation surfaced to diagnose a "hangs with a large
corpus + Stop/Pause unresponsive" report. No behavioral changes; diagnostics-only.

1. `ScreeningProgress.phase: Option<String>` carries the coarse run-phase
   (`"preparing:translating"` / `"preparing:chunking"` / `"screening"` /
   `"stage2"`); the frontend progress bar renders it as the sub-line during
   prep phases so the user sees "Extracting full-text chunks…" instead of a
   silent 0% freeze. `#[serde(default)]` so old payloads still deserialize.
2. `log_diag!` macro (always-on, NOT gated on `cfg(debug_assertions)` like
   `debug_log!`) emits `[screening:diag]` lines: phase transitions, per-batch
   `batch_start`, stage-1/stage-2 cancel detection (`llm_call: cancel
   detected…`), `stop_screening: IPC received`, orchestrator
   `LLM call START/END/TIMEOUT`, and a 5s `HEARTBEAT` (exits on
   `is_running==false || cancel_token==true` so it never leaks past the run).
   Run as `Bango 2>screening.log` and `grep screening:diag`.
3. Phase B (chunk backfill) progress callback:
   `ensure_chunks_for_full_text_articles_with_progress` invokes a
   `ChunkProgressCb` per article; the screening task emits a
   `screening:progress` event + `chunk_progress: done/total` log line per
   article. **The lock pattern is UNCHANGED** - `db.conn.lock()` is still held
   across the whole pass exactly as today; the callback only emits events
   between articles.
4. `connection.rs::lock_conn` times the acquire and emits
   `lock_conn: SLOW acquire ({ms}ms)` when > `SLOW_LOCK_THRESHOLD_MS = 100`.
5. `translation/wait.rs` emits `translation_wait: START/DONE/TIMEOUT` per
   article (no-op when `auto_translate=false`, the opt-in default).

Decision table + run instructions in `.worktrees/diagnostic1.md`.

### Three modes

`engine.rs` adds `ScreeningMode` (`Abstract`/`Enhanced`/`TwoStage`) +
`ScreeningConfig` (mode, `enhanced_top_k`, `enhanced_sections`,
`two_stage_low`/`high`, `chunk_budget_per_article`, optional `max_articles`
per-run cap from `start_screening(max_articles)`); `run_sync` gains a `config`
param.

- **Enhanced**: per article with `has_full_text=1`, retrieves top-K chunks →
  `rank_chunks_by_criteria` → `format_chunks_as_evidence` (pure `#[must_use]`)
  → attaches as `ArticleEntry.full_text_evidence`; one batched LLM call
  categorized as `LlmRequestType::EnhancedScreening`.
- **Two-stage**: stage 1 abstract-only; borderline articles
  (`two_stage_low <= conf < two_stage_high`, default `[0.4,0.7)`) with full
  text get a second per-article evidence call that overrides stage 1; both
  passes flow through `resolve_decision` and write audit entries (`ai_screen`
  stage 1, `ai_screen_enhanced` stage 2). `ScreeningProgress` gains
  `stage`/`stage_total` for the progress sub-line.
- **Always-selectable mode + per-article fallback**: all three modes are
  selectable in Settings regardless of attachments/articles; Enhanced and
  Two-stage evidence retrieval is applied per article only when
  `has_full_text=1` and the run falls back to abstract-only screening
  otherwise (the engine already degrades per-article; the Settings UI no
  longer gates selection on `full_text_article_count >= 1`).

`prompt.rs` `ArticleEntry` gains `full_text_evidence: Option<String>`; the
prompt emits a `## Supporting Evidence from Full Text` block (chunks prefixed
`[§Methods]`/`[§Results]`) only when `Some` (abstract-mode prompts stay
byte-identical). `prompt.rs` `SYSTEM_PROMPT` carries a `## Tag and Label
Guidelines` section (v6.9). `llm_client.rs` gains a non-breaking
`send_with_type(system, user, LlmRequestType)` default method (delegates to
`send`); only `HttpLlmClient` overrides it to route the type through the
orchestrator.

### Tag/label sanitization (v6.9)

`engine.rs` exposes pure `#[must_use]` `sanitize_tag_or_label_name(raw,
max_len)` + `truncate_at_word_boundary(s, max_len)`. The sanitizer strips
`inclusion:`/`exclusion:`/`inclusion -`/`exclusion -` prefixes, lowercases,
replaces spaces/underscores with hyphens, collapses repeated hyphens, trims
leading/trailing hyphens, and truncates at the last word boundary (never
mid-word); a single overlong word with no hyphens hard-truncates at the limit.
`MAX_NEW_TAG_LABEL_LEN = 35` (raised from 30). Both `create_or_match_tag` and
`create_or_match_label` route through the sanitizer so auto-generated criterion
labels (`"Inclusion: {text}"`) no longer leak the prefix into the stored name.

### Stage-2 progress + token accumulation + accurate audit label

- **Stage-2 progress**: every early-exit arm in the two-stage loop (evidence
  filtered out, LLM error, parse mismatch, `"error"` decision) updates
  `ScreeningProgress.stage` so the `X/Y borderline` sub-line never stalls.
- **Token accumulation**: `update_article_after_screening` writes
  `actual_tokens = COALESCE(actual_tokens, 0) + ?` so the stage-2 cost adds to
  (not overwrites) stage-1 for borderline articles.
- **Accurate enhanced audit label**: the evidence-sections label written to the
  `ai_screen_enhanced` audit detail is captured during retrieval
  (`ArticleEvidence.sections_label`, the sections that *actually* matched), not
  derived from the configured allow-list.
- **Mode-aware token estimation** (Gap 5): `token_estimation::
  worst_case_per_article_tokens` (pure, `#[must_use]`) computes the §4.3
  worst-case footprint per active mode (Abstract = abstract+template; Enhanced
  adds `chunk_budget/4`; Two-stage adds `chunk_budget/4 *
  two_stage_expected_borderline_fraction`); both `get_screening_readiness` and
  `estimate_screening_tokens` route through it so their estimates stay in sync.

### Commands (`commands/screening.rs`)

Reads mode + params from `app_settings` and runs
`ensure_chunks_for_full_text_articles(conn, force=false)` inside the spawned
background task before `run_sync` (NOT in the synchronous IPC handler, so the
PDF-parse + chunk-write pass does not freeze the UI by holding the DbState
mutex); the Settings "Rebuild text chunks" button calls the same fn with
`force=true` so a corrupted/partial/outdated chunk set is repaired. Exposes
`get_screening_mode`/`set_screening_mode`/`get_full_text_article_count`
commands. The `commands/screening.rs` honors `1..=15` verbatim for `batch_size`,
matching the frontend stepper's `BATCH_MAX`.

## Work Guidance

- When modifying cancel/timeout behavior, preserve the always-poll-notified +
  check-token-inside pattern (see v8.5 above).
- When adding a new request type, extend `timeout_for` in
  `llm::orchestrator` (see `llm/AGENTS.md`).
- The tag/label sanitizer is the single path for auto-generated names; do not
  bypass it.

## Verification

- `tests/decision_test.rs`, `tests/error_classify_test.rs`,
  `tests/json_parse_test.rs`, `tests/article_writer_test.rs`,
  `tests/screening_e2e_test.rs`, `tests/screening_two_stage_test.rs`,
  `tests/screening_engine_test.rs`, `tests/resolution_test.rs`,
  `tests/screening_prompt_test.rs`, `tests/token_estimation_test.rs`,
  `tests/chunk_retrieval_test.rs`, `tests/article_query_test.rs`.
- Inline tests in each `engine/` submodule (types serde, prompt-parts cloning,
  `is_borderline`, `is_transient_llm_error`, `is_auth_failure`,
  `stop_during_request_delay_*`, sanitize + truncate + create_or_match edge
  cases, system-prompt guidelines, chunk-progress callback, phase field serde).
- `tests/llm_orchestrator_test.rs` (3 `timeout_for` tests).

## Child DOX Index

- **`engine/`** - directory module (`mod.rs` + `types.rs` + `prompt_parts.rs`
  + `stage2.rs`). No own `AGENTS.md`; the contracts above cover the engine.