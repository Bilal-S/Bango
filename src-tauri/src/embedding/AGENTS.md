# embedding/

## Purpose

Semantic article search. Generates and stores per-article, per-chunk embedding
vectors, and recalls the most semantically similar articles/passages for
downstream features (Citation Finder, chat RAG). The provider client +
orchestrator routing live in `llm/embedding.rs` + `llm/orchestrator.rs` (see
`llm/AGENTS.md`); this module owns the director + runner + recall + pure
text/batching helpers, plus the storage layer in `db/embedding_repo.rs` (see
`db/AGENTS.md` for the `article_embeddings` schema + v007 migration).

## Ownership

- Owns: `director.rs` (orchestrates one-shot + incremental generation across
  the corpus), `runner.rs` (per-article parallel vector generation +
  DB-write), `recall.rs` (similarity search over stored vectors), `text.rs`
  (pure helpers: `format_embedding_text`, `hash_text`, `expected_rows`,
  `cosine_similarity`, `serialize`/`deserialize`), `batching.rs`
  (`group_into_embedding_batches`, `split_text_by_token_budget`), `mod.rs`.
- Consumed by: `commands/embedding.rs` (the `recall` command + generation
  triggers), `commands/summary.rs` (post-summary fire-and-forget),
  `commands/full_text.rs` (rebuild-text-chunks cascade),
  `batch_import/embeddings_phase.rs` (Phase 5), `citation_finder/search.rs`
  (embedding prefilter reuses `recall::recall`), and the Test Connection probe
  (`commands/llm_config.rs`).

## Local Contracts

### Runner v2 redesign (see `.worktrees/embed-v2-handoff.md` + `.worktrees/embed-plan.md` v2 addendum)

The runner is an OUTER `tokio::task::JoinSet` (one task per article) instead of
a sequential `for` loop. Each task calls
`EmbeddingBatchSender::send_embedding_batch_parallel` (injectable trait, mirrors
`IngestLlmSender`; production `HttpEmbeddingBatchSender` wraps the orchestrator;
tests inject a fake), then writes its rows under a brief DB lock burst.
Cancellation is via `JoinSet::abort_all`: a Cancel click between
`join_next()` completions aborts all in-flight tasks, dropping their vectors
(no DB writes from cancelled tasks). The v1 Phase 5 mirror task (polling
`cancel_handle` every 100ms to forward to an atomic) was REMOVED - the outer
`abort_all` makes it obsolete. Phase 5 now snapshots `cancel_handle` into an
`Arc<AtomicBool>` ONCE before calling the runner.

The runner accepts an optional `cancel_token: Option<Arc<AtomicBool>>` (checked
between `join_next()` completions).

### Per-row dimension guard

The runner validates per-row dimension consistency via two pure `#[must_use]`
helpers (`resolve_effective_dim`, `vector_matches_dim`): a provider returning
vectors of an unexpected length (model swap, truncated batch) no longer silently
stores a wrong `dimensions` column - the effective dim tracks the provider's
reported value (with drift persisted back to `app_settings`), and any per-row
mismatch is skipped + counted as an error.

### Lock discipline

DB mutex is NEVER held across an `.await`. Three brief lock bursts - (1) read
work list + config + status, (2) persist probe outcome if `unknown`, (3)
per-completed-article `INSERT OR REPLACE` - with the embedding HTTP calls
happening lock-free between bursts.

### v2 orchestrator primitives

`send_batch_parallel` (generic, free function - order-preserving parallel
dispatch via JoinSet with panic isolation) + `send_embedding_batch_parallel`
(embedding-specific: per-text splitting via `split_text_by_token_budget` +
sub-batch grouping via `group_into_embedding_batches` + parallel HTTP dispatch
+ token-weighted mean-pooling via `pool_vectors`). Both are FREE functions (not
`&self` methods) because `JoinSet::spawn` requires `'static` futures. The 4
callers (`commands/embedding.rs`, `commands/summary.rs`,
`commands/full_text.rs`, `batch_import/embeddings_phase.rs`) each wrap the
orchestrator into `HttpEmbeddingBatchSender` at the call site.

## Work Guidance

- Inject `Arc<dyn EmbeddingBatchSender>` into the runner so the parallel +
  cancel behavior is unit-testable without a live provider.
- Never hold the DB mutex across an `.await`; use the three-burst lock pattern.
- The `-1` chunk_index sentinel for the title+abstract row is owned by the
  storage layer (`db/embedding_repo.rs`).

## Verification

- `tests/embedding/embedding_storage_test.rs` (11)
- `tests/embedding/embedding_text_test.rs` (5)
- `tests/embedding/embedding_provider_test.rs` (19)
- `tests/embedding/embedding_director_test.rs` (10)
- `tests/embedding/embedding_recall_test.rs` (7, incl. the `f32::NEG_INFINITY`
  max-pool sentinel regression test)
- `tests/embedding/embedding_runner_test.rs` (9, covering the pure `resolve_effective_dim`
  + `vector_matches_dim` helpers that drive the runner's per-row dimension
  validation)
- `tests/embedding/embedding_probe_persist_test.rs` (13, covering the Test Connection
  probe dimension-forwarding contract + the `save_llm_config`
  conditional-reset contract - `embedding_relevant_changed` - which ensures a
  parameters-only save does NOT wipe a known-good `embedding_status = enabled`,
  preventing the redundant Phase B probe on the next Citation Finder run)
- `tests/llm/llm_orchestrator_batch_test.rs` (17: `send_batch_parallel`
  order/mixed/panic/empty + `send_embedding_batch_parallel` mockito dispatch +
  per-provider limits table)
- `embedding/batching.rs` inline (7: `group_into_embedding_batches` bin-pack
  respecting both caps)

85 tests total.

## Child DOX Index

No child `AGENTS.md` files. This module owns `director.rs`, `runner.rs`,
`recall.rs`, `text.rs`, `batching.rs`, `mod.rs` with no further durable
boundaries. The provider client lives in `llm/embedding.rs` (see
`llm/AGENTS.md`); the storage layer lives in `db/embedding_repo.rs` (see
`db/AGENTS.md`).