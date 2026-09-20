# llm/

## Purpose

OpenAI-compatible + native Anthropic Messages API + Google Generative Language
chat-completion client and the
centralized LLM request orchestrator. All LLM calls in the app MUST flow
through `LlmOrchestrator` (per `docs/CLAUDE.md`), which enforces concurrency
limits + rate limiting and delegates to `client::send_chat_completion`.

## Ownership

- Owns: `client.rs` (HTTP + retry + payload normalization + response parsing),
  `orchestrator.rs` (concurrency semaphore + rate limiting + `LlmRequestType`
  categorization + `send_embedding` + `send_batch_parallel` + `send_embedding_batch_parallel`),
  `embedding.rs` (per-provider embedding HTTP client + capability probe + per-provider limits),
  `backend.rs` (the `LlmBackend` domain type - leaf module), `local/` (Bango AI
  profile/policy/hardware/pinned-manifest/install/engine manager: spawn +
  health are trait seams, single-flight startup, per-start API key, native
  `--sleep-idle-seconds` idle, in-flight accounting; start attempts are
  budgeted separately from the crash budget, `Failed` is sticky until an
  explicit stop/reset, stops are generation-guarded, and the reserved loopback
  port is held by a listener until the spawn hands it over),
  `effective_config.rs` (backend-aware config resolver used by every
  generation call site for prompt budgeting), `readiness.rs`
  (`has_usable_llm` + the path-aware `embedding_generation_ready`), `mod.rs`.
- Consumed by every feature that makes LLM calls: screening, summaries, tags,
  labels, criteria, chat, wiki ingest/chat, translation, gap analysis, search
  strategy, OpenAlex smart search, figure descriptions, and embedding generation
  (the `embedding/` module calls `send_embedding` / `send_embedding_batch_parallel`
  through the orchestrator, never `client::embed_texts` directly).

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
- **Client-level retry inside the timeout envelope**: all three provider paths
  (`send_openai_compatible`, `send_google`, `send_anthropic`) wrap their
  request-build + send +
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
- **`CallMeta.finish_reason` truncation flag** (wikifix-final Change 4): every
  provider path parses its stop reason (`finish_reason` OpenAI, `stop_reason`
  Anthropic, `finishReason` Google) into `CallMeta.finish_reason`;
  `truncated_by_output_budget()` is `true` for `length` / `max_tokens` /
  `MAX_TOKENS`. `LlmOrchestrator::send_with_meta` exposes the meta to callers
  (wiki ingest); plain `send` still drops it. No output-cap field is EVER sent
  on the OpenAI-compatible or Google paths (user ruling: no generic output
  restriction); Anthropic keeps the ceiling-not-target `max_tokens` + back-down.
- **`estimated_output_budget_tokens(config)`**: planning-only estimate
  (per-model table, Anthropic latched-request ceiling, conservative defaults
  for local endpoints). Used solely for wiki batch sizing; never sent.
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
- **`test_connection` owns its temperature persistence**: it returns
  `(String, usize, CallMeta)` and flips the
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
- Tested in `tests/llm/llm_client_test.rs` (4 temperature tests: recovery retry,
  skip-when-already-skipping, no-retry-non-temperature, default-CallMeta-on-
  success) + `tests/llm/llm_orchestrator_test.rs` (2 persistence tests: fires-on-
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

### Native Anthropic Messages API path (`client.rs`)

`LlmProvider::Anthropic` routes to `send_anthropic` (like Google, it is NOT
OpenAI-compatible). Routing an OpenAI-shaped body to
`https://api.anthropic.com/v1/messages` fails: first with
`anthropic-version: header is required`, then with body-shape 400s.

- **Headers**: every request carries `anthropic-version: 2023-06-01` (the
  latest and only non-deprecated API version, REQUIRED unconditionally - this
  is why the header is always sent, not flag-gated like `skip_temperature`)
  and `x-api-key` auth (Bearer is also accepted by the API; `x-api-key`
  matches the `list_models` path).
- **Body**: `system` prompt is a TOP-LEVEL field (the Messages API has no
  `"system"` role in `messages`), a single `user` message, and the REQUIRED
  `max_tokens` field. Output-cap capability probe: requests ask for
  `ANTHROPIC_REQUESTED_MAX_TOKENS = 32_768` first; an over-cap 400 (whose
  message states the model's true limit, `max_tokens: N > M, which is the
  maximum allowed...`) backs down to the parsed `M`, falls back to
  `ANTHROPIC_SAFE_MAX_TOKENS = 4096` when unparseable, latches the cap per
  model name in `ANTHROPIC_CAP_CACHE` (session-scoped), and retries exactly
  once (a second over-cap surfaces as-is - loop guard). Back-downs surface
  via `CallMeta.max_tokens_backed_down`, which Test Connection appends to its
  success message.
- **Response**: text is the concatenation of every `type: "text"` content
  block (non-text blocks like `tool_use` carry no `text` and are skipped);
  the token total is `usage.input_tokens + usage.output_tokens`. Empty text
  surfaces the standard `No response from LLM` error. `stop_reason ==
  "max_tokens"` logs a truncation diagnostic (parity with the OpenAI path's
  `finish_reason == "length"` handling).
- Temperature-rejection recovery applies unchanged inside every envelope
  (`send_with_temperature_recovery` via `anthropic_attempt`); a recovery in
  the backed-down envelope still sets `CallMeta.temperature_was_rejected`.
- Frontend complement: `src/utils/llm-error.ts` maps `anthropic-version`
  errors to the `anthropic-version-missing` troubleshooting anchor
  (`help-tab-troubleshooting.vue`) for Custom endpoints proxied to Anthropic.
- Tested in `tests/llm/llm_client_test.rs` (13 tests: required headers, native
  request shape, multi-block join, missing API key, direct `/messages`
  endpoint, empty content, temperature recovery, back-down to reported cap,
  unparseable-body 4096 fallback, reported-8192 win, per-model latch,
  persistent-over-cap loop guard, stop_reason truncation) plus inline units
  for `is_over_cap_error` / `parse_model_cap`.

### Embeddings (`embedding.rs` + `orchestrator.rs`)

- **Provider embedding client** (`embedding.rs`): per-provider HTTP shapes for
  the `/embeddings` endpoint. OpenAI-compatible providers (OpenAI, Mistral, LM
  Studio, llama.cpp, Custom, Z.AI tried at runtime) send `{"model","input":[…]}`
  and parse `data[*].embedding`; Ollama loops one `/api/embeddings` call per
  text (`{"model","prompt"}`); Google uses `models/{model}:embedContent`. Routes
  through `client::shared_client()` for HTTP keep-alive but does NOT go through
  `send_chat_completion` (different endpoint + response shape).
- **Per-provider limits** (`EmbeddingLimits`): `{max_inputs_per_batch,
  max_tokens_per_input, max_tokens_per_batch}`. All three caps are respected
  simultaneously when bin-packing sub-batches. Drives
  `embedding::text::split_text_by_token_budget` (per-input) and
  `embedding::batching::group_into_embedding_batches` (sub-batch grouping). Pure
  `#[must_use]` `embedding_limits(provider, model)` is unit-tested in
  `tests/embedding/embedding_provider_test.rs`.
- **Triple-state capability flag** (`app_settings` keys `embedding_status`
  [`unknown` default | `enabled` | `disabled`], `embedding_model`,
  `embedding_dimensions`): records the outcome of the last probe so
  `generate_embeddings` / `recall` can short-circuit without re-probing.
  `save_llm_config` resets `embedding_status` to `unknown` (keeps model + dims
  for the Settings UI) so a provider/endpoint/model switch re-evaluates.
- **Capability probe** (`probe_embedding_support(config) -> ProbeOutcome`):
  resolution order - (1) Anthropic → `disabled` immediately; (2) try the
  provider-default embedding model with the word `"probe"`; (3) on failure,
  retry with the configured chat model (some local servers serve embeddings
  from the loaded model); (4) both fail → `disabled`. Returns
  `{status, model, dimensions, reason}`. Runs during `Test Connection`
  (`commands::llm_config::test_llm_connection` calls `probe_embeddings_sync`
  inline so the result + model land in the response payload + toast) and on the
  first `generate_embeddings` call when `embedding_status == unknown`.
- **Dimension-forwarding contract** (regression-tested in
  `tests/embedding/embedding_probe_persist_test.rs`): the Test Connection path MUST
  forward `ProbeOutcome.dimensions` through `probe_embeddings_sync` →
  `persist_embedding_probe` → `set_embedding_status`. The prior shape hardcoded
  `dimensions = 0`, which left `recall` gated off (`dimensions <= 0` at
  `embedding/recall.rs:59`) until the first `generate_embeddings` call. The
  extracted DB-write core `persist_embedding_probe_to_conn` is `pub` so
  integration tests can exercise the contract without a Tauri `State<DbState>`.
- **Routing through the orchestrator**: `LlmOrchestrator::send_embedding`
  acquires the semaphore + rate limit + 30s `LlmRequestType::Embedding` timeout,
  then delegates to `llm::embedding::embed_texts`. The free functions
  `send_batch_parallel` (generic, order-preserving JoinSet with panic isolation)
  and `send_embedding_batch_parallel` (embedding-specific: per-text split →
  sub-batch group → parallel HTTP → token-weighted mean-pool) are FREE
  functions (not `&self` methods) because `JoinSet::spawn` requires `'static`
  futures; callers wrap the orchestrator into the backend-aware
  `BackendEmbeddingBatchSender` at the call site (`runner::backend_sender`). The runner takes `Arc<dyn EmbeddingBatchSender>` (injectable trait,
  mirrors `IngestLlmSender`) so its parallel + cancel behavior is unit-testable
  without a live provider.
- **Lock discipline** (the runner): DB mutex is NEVER held across an `.await`.
  Three brief lock bursts - (1) read work list + config + status, (2) persist
  probe outcome if `unknown`, (3) per-completed-article `INSERT OR REPLACE` -
  with the embedding HTTP calls happening lock-free between bursts.
- **Per-row dimension guard**: `resolve_effective_dim(probe_dim, returned_dim)`
  trusts the provider on drift (keeps probe when returned is 0);
  `vector_matches_dim(vector, effective_dim)` skips + counts as error any vector
  whose length mismatches so a truncated/mismatched vector never stores a wrong
  `dimensions` column. Drift is persisted back to `app_settings`. Both are pure
  `#[must_use]`, unit-tested in `tests/embedding/embedding_runner_test.rs`.
- **Categorization**: `LlmRequestType::Embedding`; `timeout_for(Embedding) =
  30s`. Embeddings do NOT participate in the `skip_temperature` machinery (no
  temperature parameter).
- Tested in `tests/embedding/embedding_provider_test.rs` (19: model resolution,
  OpenAI-batch parse, Ollama single-prompt, Google embedContent, probe outcomes),
  `tests/llm/llm_orchestrator_batch_test.rs` (17: `send_batch_parallel` order/mixed/
  panic/empty + `send_embedding_batch_parallel` mockito dispatch + per-provider
  limits table), `tests/embedding/embedding_probe_persist_test.rs` (4: dimension-forwarding
  regression).

### Bango AI backend selection (`backend.rs` + `local/`)

- `LlmBackend` (`configured_provider` default | `bango_ai`) mirrors the
  embedding backend enum: strict `parse_exact` for command arguments,
  forgiving `parse` for DB reads. Persistence lives in
  `db::app_settings_repo::LLM_BACKEND_KEY` (project-portable; readiness stays
  machine-evaluated).
- `local/profile.rs` pins the profile identity
  (`builtin/qwen3.5-2b-ud-q4kxl@r1`), the model file name, the runtime
  directory (`llama.cpp`), the engine label, and the platform server binary
  name.
- `local/policy.rs` owns the RAM-aware context default (16k below 16 GB, 32k
  at 16 GB+, 64k at 32 GB+; calibrated for the pinned hybrid-attention 2B
  whose 64k KV cache stays under 1 GB), context clamping to 8k/16k/32k/64k,
  and the generation thread budget (`cores - 2`, floor 1, ceiling 12).
- `local/hardware.rs` owns `HardwareProfile` / `HardwareVerdict` / `assess`
  (unsupported = target or disk; warning = RAM floors or missing AVX2) plus
  the `sysinfo`-backed `detect()`.
  The AVX2 probe must live in two `#[cfg(...)]` expression blocks
  (`any(target_arch = "x86", target_arch = "x86_64")` plus the `not(...)`
  complement).
  Never guard it with a `cfg!()` runtime `if`: `is_x86_feature_detected!`
  still expands on ARM targets and breaks the macOS release build
  (regression: the aarch64 `macos-latest` CI job failed compiling exactly
  this macro before the cfg-block fix).
- `local/manifest.rs` pins Qwen3.5-2B UD-Q4_K_XL (Apache-2.0, commit-pinned
  unsloth dynamic quant of `Qwen/Qwen3.5-2B`; the previous Ornith-1.5-9B pin
  is kept as a comment for revert) and the llama.cpp `b10964` runtime
  archives with member/alias sets enumerated from the real archives during
  the T4 spike; shared scheme/hash/disk-math primitives come from `local_ai`.
  The engine spawns with `--flash-attn on`, `--batch-size 1024`, and
  `--ubatch-size 1024` (validated against the pinned runtime), and threads
  default to `cores - 2` capped at 12.
- `local/install.rs` owns the runtime/model assessment, full verification,
  the runtime-first bundle install (idempotent, shared downloader + promote),
  the model-profile install, and combined removal (locked Windows trees are
  renamed aside and swept later).
- `local/engine.rs` owns the managed `Arc<BangoAiEngine>`: a loopback port
  reserved at construction so callers get a complete config before start, lazy
  start with a per-start random `--api-key`, `/health` polling, one crash
  restart per failure streak, `stop`/`reset_off_thread` (bounded in-flight
  drain), `kill_blocking` for exit/`Drop`, and busy accounting. `ServerSpec`,
  `ServerSpawner`, `ServerProcess`, and `HealthProbe` are trait seams so
  lifecycle tests run without a real binary; the child's stdout/stderr append
  to the engine log (`--log-file` is broken in b10964). Drain contract:
  `DrainTimings { fast, max }` is injectable (production 5 s fast / 30 min
  detached cap, `LOCAL_DRAIN_MAX_SECS`), deliberately independent of the 60 min
  request budget, and the loop deadlines use `tokio::time::Instant` so paused
  tokio time drives them in tests. After the cap the detached
  `stop_if_unchanged` kills the server and the in-flight request fails with a
  transport error. Lock rule: the inner
  mutex is NOT reentrant - guard holders must build configs with the private
  `config_from`, never `effective_config` (the T5c self-deadlock fix).
- `effective_config.rs` (T6) is the resolver: `resolve` / `resolve_no_decrypt`
  return the stored cloud row under `configured_provider` or a complete local
  config (reserved port + recommended machine settings, temperature 0.6,
  concurrency 1, delay 0) under `bango_ai`; `effective_context_window` +
  `is_local_backend` (both in `effective_config.rs`) are the kept helper
  surface for window/budget consumers and the routing tests (aifixes1 D2
  decision: kept, not deleted).
- `readiness.rs` (T6) owns `has_usable_llm` (cloud row, or installed Bango AI
  components) and `embedding_generation_ready` (path-aware: the cloud
  embedding branch still needs a cloud row even when Bango AI serves
  generation).
- `LlmProvider::BangoAi` is runtime-only: `save_config` rejects it and
  `parse_provider` never produces it, so the `llm_config` CHECK constraint is
  untouched.
- Orchestrator local routing (T6): `set_backend`/`set_backend_initial` +
  `set_local_config_provider` (engine-backed `EngineLocalConfigProvider`);
  `send_with_meta_opts` substitutes the live local config, applies
  `resolve_timeout` (single 3600 s local override, because many CPUs are
  slow), skips the cloud
  temperature latch/persistence, and builds `client::RequestOptions` via
  `local_request_options` (structured calls force `enable_thinking=false`;
  prose honors the toggle). `send_opts(json_mode)` is the JSON-intent seam
  (`send_json` and the screening `HttpLlmClient` use it); cloud transports
  never receive `response_format`/`chat_template_kwargs`.
- The engine (`local/engine.rs`, T5) and the effective-config resolver
  (`effective_config.rs`, T6) consume these modules; `has_usable_llm` becomes
  the backend-aware generation gate in T6.

### `send_json` + JSON pre-parser (`orchestrator.rs` + `utils/json_repair.rs`)

- `LlmOrchestrator::send_json` is the canonical entry point for any caller that feeds the LLM response into `serde_json::from_str`. It chains `send` (concurrency + rate limit + timeout + temperature recovery) with `utils::json_repair::prepare_llm_json`, which strips markdown code fences and escapes raw control characters (`0x00`-`0x1F`) that the LLM may place inside JSON string values.
- **Contract**: JSON-returning LLM consumers (article summary, section summary, figure descriptions, criteria generation, OpenAlex smart search, search strategy, unified summary) MUST use `send_json`. The response `String` is ready for `serde_json::from_str` without any further cleanup.
- **Prose callers** (chat, wiki chat, literature review, wiki ingest, markdown-fallback retry, translation) MUST use `send` instead - running the JSON pre-parser on prose would corrupt quoted spans.
- **Screening** uses `send` (not `send_json`) because its `extract_json` does array-specific shape repair that the generic pre-parser cannot handle; the screening path runs `prepare_llm_json` as the first step inside `screening::engine::extract_json` instead.
- **Local `json_object` grammars cannot emit a bare top-level array** (`response_format` engages only on the `bango_ai` path; cloud transports never see the field). Every local structured consumer must therefore request an object wrapper AND tolerate the observed object shapes:
  - Screening (`screening/prompt.rs` + `json_parse.rs`): the prompt requests `{"results": [...]}`; `process_screening_responses` recovers a wrapper key, a flat single decision object, a numeric-key map (`{"0": {...}}`), or an array-of-objects property before erroring, and keeps the bare-array + truncated-array repairs for cloud.
  - Figure descriptions (`summary/prompt.rs`): the prompt requests `{"descriptions": [...]}`; `parse_figure_descriptions_response` recovers the same object shapes.
  - Citation Finder / claim splitter already request `{"results": [...]}` / `{"claims": [...]}` and share the lenient `resolve_array` recovery (`citation_finder/prompt.rs`).
  Any new local structured call site must follow this pattern; a bare-array-only parser is a guaranteed live failure on Bango AI.
- **Local output caps (v9)**: every `bango_ai` call carries `max_tokens` derived from
  `orchestrator::local_max_tokens_for(request_type)` (WikiIngest 8192, corpus reports /
  CitationFinder 4096, chat 4096, summary types 3072, classification/structured 2048;
  embeddings uncapped). This bounds no-EOS/runaway CPU generations (a live wiki batch hit
  26K tokens with no EOS); the client sends the field only for `LlmProvider::BangoAi`, so
  cloud paths never receive it. Prose calls with the reasoning toggle ON get 2x headroom
  because thinking tokens count against the completion cap. A cap hit is visible through
  `CallMeta::truncated_by_output_budget()`, which the wiki ingest consumes (drop the partial
  trailing page + bounded continuation). `RequestOptions::max_tokens` defaults to `None`.
  Wiki batch sizing floors its planning budget to the WikiIngest cap
  (`wiki/ingest/batching.rs::wiki_batch_output_budget`) so 2 sources share a batch instead of
  singleton batches re-processing the prompt prefix. This supersedes the earlier "the local
  path sends no output cap" ruling.
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
  an integration test in `tests/llm/llm_client_test.rs`.
- No `unwrap()`/`expect()`/`panic!()` in this module (production code). The
  shared client uses `unwrap_or_else(|_| reqwest::Client::new())` because a
  builder failure should degrade to the default client, never crash.

## Verification

- `cargo test --lib llm::client::tests` - 20 inline unit tests covering
  `normalize_llm_text`, `is_retryable_response`, `calculate_backoff`,
  `is_temperature_error`, `is_over_cap_error`, and `parse_model_cap`.
- `cargo test --test llm` - 55 integration tests against a mockito
  HTTP server, including:
  - `test_openai_insufficient_permissions_403_is_retried_then_succeeds`
    (regression for the Windows-only intermittent gateway error),
  - `test_openai_real_auth_401_is_not_retried` (plain 401 fails fast),
  - updated `test_*_rate_limit_429` / `test_*_server_error` cases asserting
    4 attempts (1 + 3 retries) per the retry contract,
  - 4 temperature-recovery tests (`test_openai_temperature_400_retries_without_temperature`,
    `test_openai_temperature_400_with_skip_temperature_true_does_not_retry`,
    `test_openai_nontemperature_400_does_not_retry`,
    `test_openai_success_returns_default_callmeta`),
  - 13 native Anthropic Messages API tests (headers, request shape,
    multi-block join, missing key, direct endpoint, empty content,
    temperature recovery, over-cap back-down to reported limit,
    unparseable-body 4096 fallback, reported-8192 win, per-model cap latch,
    persistent-over-cap loop guard, stop_reason truncation - see the
    Anthropic path contract above).
- `cargo test --test llm_orchestrator_test` - 40 orchestrator tests including 2
  temperature-persistence tests (`temperature_persister_fires_on_recovery`,
  `temperature_persister_does_not_fire_on_normal_success`), 1 in-session
  latch test (`session_latch_skips_temperature_on_second_call_after_first_rejection`),
  and 1 Test Connection regression test
  (`test_connection_surfaces_temperature_recovery_and_latches`).

## Child DOX Index

No child `AGENTS.md` files. This module owns four files (`mod.rs`,
`client.rs`, `orchestrator.rs`, `embedding.rs`) with no further durable
boundaries. The consumer-facing `embedding/` module (director, runner, recall,
text, batching) lives at `src-tauri/src/embedding/` and is documented in the
root `AGENTS.md` Child DOX Index.
