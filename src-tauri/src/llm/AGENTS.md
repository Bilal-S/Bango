# llm/

## Purpose

OpenAI-compatible + Google Generative Language chat-completion client and the
centralized LLM request orchestrator. All LLM calls in the app MUST flow
through `LlmOrchestrator` (per `docs/CLAUDE.md`), which enforces concurrency
limits + rate limiting and delegates to `client::send_chat_completion`.

## Ownership

- Owns: `client.rs` (HTTP + retry + payload normalization + response parsing),
  `orchestrator.rs` (concurrency semaphore + rate limiting + `LlmRequestType`
  categorization), `mod.rs`.
- Consumed by every feature that makes LLM calls: screening, summaries, tags,
  labels, criteria, chat, wiki ingest/chat, translation, gap analysis, search
  strategy, OpenAlex smart search, figure descriptions.

## Local Contracts

### Retry + transient-failure handling (`client.rs`)

- `send_with_retry` owns bounded retry on both transport errors AND non-2xx
  HTTP responses. `MAX_RETRIES = 3` (4 total attempts); exponential backoff
  `1s -> 2s -> 4s` capped at `MAX_BACKOFF_MS = 10_000` with 0-500ms jitter
  (mirrors `openalex::client::calculate_backoff`). The `Retry-After` header is
  honored (delta-seconds form, capped at `MAX_BACKOFF_MS`) when present.
- `is_retryable_response(status, body)` decides what gets retried:
  - Always retry: `429`, `408`, `5xx`.
  - Conditionally retry: `401`/`403` ONLY when the body contains the exact
    string `"insufficient permissions for this operation"`. This signature is
    an empirically-observed OpenAI/Cloudflare project-scope transient that
    succeeds on resubmit (Windows-only intermittency). Gating on the body
    ensures real auth failures (wrong/revoked key, wrong org) fail fast after
    one attempt instead of burning retry budget.
  - Never retry: `400`, `404`, and plain `401`/`403` without the gated body.
- The orchestrator's `tokio::time::timeout` bounds the FULL retry sequence,
  not a single attempt. The timeout is per-`LlmRequestType` (v8.2): 120s for
  `Screening`/`EnhancedScreening`, 600s for all other request types. The
  shared client sets only connect/pool timeouts, no per-request timeout.
- The full retry rationale (why the Windows 401/403 transient-body gate exists
  and why it must NOT be removed) is documented in the `is_retryable_response`
  doc-comment in `client.rs` - read it before modifying the retry policy.
- A distinct `eprintln!("[LlmClient] retrying Windows transient ...")` log
  line fires when the transient-body gate matches, so users can confirm the
  workaround is engaging in production diagnostics.

### Temperature-rejection recovery (`client.rs` + `orchestrator.rs`)

- Models that only support the default `temperature` (typically `1`) reject a
  non-default value with HTTP 400 + a body whose `message` mentions
  `temperature` plus `unsupported` / `does not support` / `not supported`. The
  pure `#[must_use]` helper `client::is_temperature_error(msg)` classifies
  these. It deliberately does NOT match the bare word `invalid` (which appears
  in out-of-range errors like `"temperature parameter is invalid"` that should
  NOT trigger retry-without-temperature). 6 inline unit tests in
  `client::tests` cover the OpenAI + Google shapes plus negative cases.
- **Client-level retry inside the timeout envelope**: both provider paths
  (`send_openai_compatible`, `send_google`) wrap their request-build + send +
  parse logic in `send_with_temperature_recovery(skip_temperature, temperature,
  make_request)`. On a temperature-rejection 400, it rebuilds the request with
  `temperature = None` and calls `make_request` once more. The retry happens
  INSIDE the client, so it shares the orchestrator's single outer
  `tokio::time::timeout` envelope - there is NO doubling of the wall-clock
  budget (the recovery call consumes whatever time remains). This is why the
  recovery lives in the client, not the orchestrator (where a naive retry
  would start a fresh timeout).
- **Recovery skipped when already skipping**: if `config.skip_temperature` is
  `true`, the first attempt already omits `temperature`, so a 400 cannot be a
  temperature rejection - the error surfaces immediately.
- **Original error preserved on second-attempt failure**: if the recovery call
  also fails, the ORIGINAL (temperature-specific) error is returned so the
  caller sees the actionable diagnostic.
- **`CallMeta` side-channel**: `send_chat_completion` returns
  `(String, usize, CallMeta)` where `CallMeta.temperature_was_rejected` is
  `true` iff the call recovered from a temperature 400. The orchestrator
  inspects this flag and persists the flag; callers see only `(String, usize)`.
- **Orchestrator post-call persistence**: `LlmOrchestrator::send` calls
  `maybe_persist_skip_temperature(meta)` after a successful call. If the flag
  is set, it (a) latches an in-session `AtomicBool`
  (`temperature_rejected_in_session`) so every subsequent call in this process
  omits `temperature` from the start (no wasteful first-attempt 400 + retry),
  and (b) spawns a detached `tokio::task::spawn_blocking` that invokes the
  wired `TemperatureFlagPersister` to persist the flag to the DB for future
  process restarts. The in-session latch is the fix for the "every screening
  batch retries temperature" bug: long-running consumers (screening engine)
  cache `LlmConfig` in memory and never re-read the DB row mid-run, so DB
  persistence alone cannot reach them. The trait decouples the LLM layer from
  `tauri::AppHandle` + `DbState`; the production impl
  (`AppHandleTemperaturePersister` in `lib.rs`) runs the targeted
  `llm_config_repo::set_skip_temperature` `UPDATE` (NOT `save_config`, which
  would `DELETE`+`INSERT` the whole row and race with concurrent UI saves).
  Best-effort: errors are logged and swallowed so a DB hiccup never fails a
  successful LLM call.
- **Deadlock-free invariant**: the persistence lock is acquired AFTER the LLM
  call returns, never before or during. Every orchestrator caller releases its
  DB lock before invoking `orchestrator.send` (spec §8.1 "lock-release-call-
  lock" worker pattern + the same discipline enforced across all command
  handlers). So the persister's lock acquisition cannot deadlock with a caller.
- **`send_unthrottled` does NOT persist**: it is used only for legacy edge
  cases and intentionally does not touch the temperature flag. `test_connection`
  DOES participate: it returns `(String, usize, CallMeta)` and flips the
  in-session latch on recovery, so `test_llm_connection` can detect the
  recovery (`Ok` + `temperature_was_rejected`) and persist
  `skip_temperature = true` to the DB. This closes the regression where the
  client-level recovery made the 400 silent, causing `test_llm_connection` to
  report success without persisting the flag.
- **Test ergonomics**: `LlmOrchestrator::new(max_conc, delay_ms)` is unchanged
  (2 params). The persister is wired via a separate
  `set_temperature_persister(Arc<dyn TemperatureFlagPersister>)` setter, so the
  ~40 existing test call sites need zero edits. `NoOpTemperaturePersister` is
  the test/default impl (no-op). Tests inject a `RecordingPersister` fake to
  assert the persistence signal fires.
- Tested in `tests/llm_client_test.rs` (4 temperature tests: recovery retry,
  skip-when-already-skipping, no-retry-non-temperature, default-CallMeta-on-
  success) + `tests/llm_orchestrator_test.rs` (2 persistence tests: fires-on-
  recovery, does-not-fire-on-normal-success).

### Shared HTTP client (`client.rs`)

- `shared_client()` returns a lazily-built, app-lifetime `reqwest::Client`
  (`OnceLock`). Reusing one client enables HTTP keep-alive so repeated LLM
  calls reuse a single TLS session instead of performing a fresh handshake
  per request. This matters on Windows (SChannel), where per-request TLS setup
  is materially more failure-prone under concurrency.
- `Client::new()` is NOT used on the chat-completion path (only `list_models`
  still uses it; it is a low-frequency discovery endpoint and acceptable).
- Only `connect_timeout(30s)` + `pool_idle_timeout(90s)` are set on the shared
  builder. No request timeout (the orchestrator owns the wall-clock cap).

### Payload normalization (`client.rs`)

- `normalize_llm_text(input) -> Cow<str>` strips `\r` and coerces NBSP
  (`\u{00A0}`) to ASCII space. Applied once at the top of both send paths to
  `system_prompt` + `user_prompt`. Fast path returns `Cow::Borrowed` (no
  allocation) when no `\r`/NBSP is present.
- This is defense-in-depth hygiene, NOT a request-correctness requirement:
  `reqwest::json` already escapes JSON control chars. NBSP slips in from PDF
  extraction; `\r` can appear in Windows-edited text. Do NOT add per-call-site
  normalization; the client is the single choke point.

### Diagnostics (`client.rs`)

- Every non-success response error string carries the OpenAI/Cloudflare trace
  IDs: ` [req=<x-request-id>, cf-ray=<CF-Ray>]` (either or both, when present).
  These are the exact IDs OpenAI support + Cloudflare need to trace a
  transient. Format: `LLM request failed (<status>) [req=..., cf-ray=...]: <body>`.
- Each retry attempt logs `[LlmClient] {label} attempt {n}/{N} failed (<status>)[trace]; retrying in {ms}ms`
  to stderr so the fix can be confirmed engaging in production logs.

### `send_json` + JSON pre-parser (`orchestrator.rs` + `utils/json_repair.rs`)

- `LlmOrchestrator::send_json` is the canonical entry point for any caller that feeds the LLM response into `serde_json::from_str`. It chains `send` (concurrency + rate limit + timeout + temperature recovery) with `utils::json_repair::prepare_llm_json`, which strips markdown code fences and escapes raw control characters (`0x00`-`0x1F`) that the LLM may place inside JSON string values.
- **Contract**: JSON-returning LLM consumers (article summary, section summary, figure descriptions, criteria generation, OpenAlex smart search, search strategy, unified summary) MUST use `send_json`. The response `String` is ready for `serde_json::from_str` without any further cleanup.
- **Prose callers** (chat, wiki chat, literature review, wiki ingest, markdown-fallback retry, translation) MUST use `send` instead - running the JSON pre-parser on prose would corrupt quoted spans.
- **Screening** uses `send` (not `send_json`) because its `extract_json` does array-specific shape repair that the generic pre-parser cannot handle; the screening path runs `prepare_llm_json` as the first step inside `screening::engine::extract_json` instead.
- Manual `strip_code_fences` calls in JSON-returning command handlers are deprecated; `send_json` handles fence-stripping centrally. The sole remaining direct `strip_code_fences` caller is the summary command's markdown-fallback retry path (prose-shaped, not JSON).

### `RequestBuilder` cloning

- `RequestBuilder` does not implement `Clone` (the body may be non-cloneable)
  but exposes `try_clone()`. Our builders always carry a serializable `.json()`
  body, so `try_clone()` returns `Some` and each retry re-issues an identical
  request. If a builder ever cannot be cloned, `send_with_retry` fails fast
  with a clear error rather than panicking or silently skipping retry.

## Work Guidance

- All LLM calls MUST go through `LlmOrchestrator` (registered as Tauri managed
  state), never `client::send_chat_completion` directly from command handlers.
- Use `LlmRequestType` to categorize every call for diagnostics.
- When adding a new retryable signature, gate it narrowly (status + body
  substring) so real permanent errors are not retried. Document it in
  `is_retryable_response` and add a unit test in the inline `tests` module +
  an integration test in `tests/llm_client_test.rs`.
- No `unwrap()`/`expect()`/`panic!()` in this module (production code). The
  shared client uses `unwrap_or_else(|_| reqwest::Client::new())` because a
  builder failure should degrade to the default client, never crash.

## Verification

- `cargo test --lib llm::client::tests` - 16 inline unit tests covering
  `normalize_llm_text`, `is_retryable_response`, `calculate_backoff`, and
  `is_temperature_error`.
- `cargo test --test llm_client_test` - 44 integration tests against a mockito
  HTTP server, including:
  - `test_openai_insufficient_permissions_403_is_retried_then_succeeds`
    (regression for the Windows-only intermittent gateway error),
  - `test_openai_real_auth_401_is_not_retried` (plain 401 fails fast),
  - updated `test_*_rate_limit_429` / `test_*_server_error` cases asserting
    4 attempts (1 + 3 retries) per the retry contract,
  - 4 temperature-recovery tests (`test_openai_temperature_400_retries_without_temperature`,
    `test_openai_temperature_400_with_skip_temperature_true_does_not_retry`,
    `test_openai_nontemperature_400_does_not_retry`,
    `test_openai_success_returns_default_callmeta`).
- `cargo test --test llm_orchestrator_test` - 40 orchestrator tests including 2
  temperature-persistence tests (`temperature_persister_fires_on_recovery`,
  `temperature_persister_does_not_fire_on_normal_success`), 1 in-session
  latch test (`session_latch_skips_temperature_on_second_call_after_first_rejection`),
  and 1 Test Connection regression test
  (`test_connection_surfaces_temperature_recovery_and_latches`).

## Child DOX Index

No child `AGENTS.md` files. This module owns three files (`mod.rs`,
`client.rs`, `orchestrator.rs`) with no further durable boundaries.