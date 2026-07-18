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
- The orchestrator's `tokio::time::timeout` (600s) bounds the FULL retry
  sequence, not a single attempt - the shared client sets only connect/pool
  timeouts, no per-request timeout.

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

- `cargo test --lib llm::client::tests` - 11 inline unit tests covering
  `normalize_llm_text`, `is_retryable_response`, and `calculate_backoff`.
- `cargo test --test llm_client_test` - 40 integration tests against a mockito
  HTTP server, including:
  - `test_openai_insufficient_permissions_403_is_retried_then_succeeds`
    (regression for the Windows-only intermittent gateway error),
  - `test_openai_real_auth_401_is_not_retried` (plain 401 fails fast),
  - updated `test_*_rate_limit_429` / `test_*_server_error` cases asserting
    4 attempts (1 + 3 retries) per the retry contract.
- `npm run check:all` runs clippy with `-D warnings` over this module.

## Child DOX Index

No child `AGENTS.md` files. This module owns three files (`mod.rs`,
`client.rs`, `orchestrator.rs`) with no further durable boundaries.