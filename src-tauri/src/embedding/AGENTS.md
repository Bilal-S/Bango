# embedding/

## Purpose

Semantic article search. Generates and stores per-article, per-chunk embedding
vectors, and recalls the most semantically similar articles/passages for
downstream features (Citation Finder, chat RAG). The provider client +
orchestrator routing live in `llm/embedding.rs` + `llm/orchestrator.rs` (see
`llm/AGENTS.md`); this module owns the director + runner + recall + pure
text/batching helpers, the backend router (`service.rs`), the local-backend
foundations (`local/`), plus the storage layer in `db/embedding_repo.rs` (see
`db/AGENTS.md` for the `article_embeddings` schema + v007 migration).

## Ownership

- Owns: `director.rs` (orchestrates one-shot + incremental generation across
  the corpus), `runner.rs` (per-article parallel vector generation +
  DB-write), `recall.rs` (similarity search over stored vectors), `text.rs`
  (pure helpers: `format_embedding_text`, `hash_text`, `expected_rows`,
  `cosine_similarity`, `serialize`/`deserialize`), `batching.rs`
  (`group_into_embedding_batches`, `split_text_by_token_budget`), `backend.rs`
  (the `EmbeddingBackend` domain type - leaf module), `service.rs`
  (the `EmbeddingService` backend router + offline local probe +
  `CloudEmbeddingProvider`), `local/` (Bango Local: `manifest.rs`
  pinned component manifest with per-file URLs + sizes + SHA-256 +
  the pinned ONNX Runtime section, `download.rs` atomic installer/verifier/
  remover with target + disk-space gates + runtime archive install +
  single-member extraction, `engine.rs` the local inference engine,
  `paths.rs` artifact-path resolution with the OneDrive fallback,
  `thread_budget.rs` CPU-cap policy, `prompt.rs` EmbeddingGemma role
  prefixes, `profile.rs` profile identity + on-disk profile dir,
  `state.rs` installation state probe), `mod.rs`.
- Consumed by: `commands/embedding.rs` (the `recall` command + generation
  triggers), `commands/summary.rs` (post-summary fire-and-forget),
  `commands/full_text.rs` (rebuild-text-chunks cascade),
  `batch_import/embeddings_phase.rs` (Phase 5), `citation_finder/search.rs`
  (embedding prefilter reuses `recall::recall`), and the Test Connection probe
  (`commands/llm_config.rs`).

## Local Contracts

### Backend router (`service.rs`) + local foundations (`local/`)

Two embedding backends, selected by the machine-local `embedding_backend`
setting (`configured_provider` default | `bango_local`; see `db/AGENTS.md`).
`EmbeddingService::embed` routes by backend + `EmbeddingRole` (Query |
Document). The service NEVER locks the DB: callers resolve `backend` +
`storage_root` inside their own brief lock burst (`recall` does this in its
existing config lock burst) and pass them in - lock discipline is preserved.
The service holds the shared `Arc<LocalEngine>` alongside the orchestrator.
The `EmbeddingProvider` trait was retired: `CloudEmbeddingProvider` is a
plain struct (the backend split lives in `EmbeddingService::embed` +
`EmbeddingBatchSender`), and `service::probe_local` is the offline probe
behind every backend-aware probe path.

- Cloud branch: `CloudEmbeddingProvider::embed` delegates both roles to
  `LlmOrchestrator::send_embedding` - byte-identical to the pre-router direct
  call, so the default backend's behavior is unchanged and the
  `docs/CLAUDE.md` orchestrator rule holds.
- Local branch: a cheap `probe_installation_state` pre-check returns the
  actionable not-installed error fast; `Ready` runs
  `LocalEngine::embed` (below). Never a silent cloud fallback.
- OneDrive fallback (`local/paths.rs`): the model root is
  `{storage_root}/model` unless the storage root path contains a OneDrive
  segment (personal or `OneDrive - <tenant>` business, whole-segment,
  case-insensitive) - then `{data_local}/Bango/ai/models` with
  `used_fallback = true`. The ONNX Runtime root is ALWAYS
  `{data_local}/Bango/ai/runtimes` (binary cache, never Documents); a missing
  app-data base degrades both to the storage root without claiming a
  fallback. Paths re-resolve from the current `storage_root` on every load -
  a moved root without an installation reports `NotInstalled`.
- Thread budget (`local/thread_budget.rs`):
  `embedding_thread_budget(cores) = clamp(cores - 2, 1, 4)` intra-op threads
  for the local session. ONNX Runtime's default is every physical core, which
  janks the UI during bulk indexing; the budget reserves 2 cores for the UI,
  async runtime, and SQLite.
- Prompt profile (`local/prompt.rs` + `local/profile.rs`): EmbeddingGemma
  requires asymmetric prefixes (`task: search result | query: ` for queries,
  `title: none | text: ` for documents); fastembed does NOT add them, Bango
  does. The profile id `builtin/embeddinggemma-300m-q4@r1` is the
  `model_name` identity, so the director's existing model-mismatch staleness
  covers prompt-strategy and artifact revisions without schema changes.

### Local engine (`local/engine.rs`)

One shared `LocalEngine` per process (managed as `Arc<LocalEngine>` in
`lib.rs`; handed to `EmbeddingService`, `recall`, the install/remove
commands, and `runner::backend_sender`). It owns a single lazily-loaded
fastembed `TextEmbedding` session behind `Arc<Mutex<Option<EngineSession>>>`:

- EVERY embed runs entirely inside one `spawn_blocking` closure that locks,
  loads on first use (state gate via `assess_installation`, ort environment,
  file bytes with `model_q4.onnx_data` as external initializer), and infers.
  No lock is held across an `.await`; async workers never block; concurrent
  callers serialize on the single session by design (the ORT intra-op pool
  is the parallelism axis).
- Inference config: role prefixes applied per call, explicit batch size 8
  (never fastembed's default 256), `LOCAL_MAX_INPUT_TOKENS` context,
  `embedding_thread_budget` intra-threads, per-vector 768-dim validation.
  A warmup embed after load is the plan-§5 self-test.
- Dylib resolution: `ORT_DYLIB_PATH` override (dev/live test) -> the
  component-manager runtime install -> actionable error. ort's process-global
  environment commits exactly once via `init_from` (under load-dynamic
  `commit()` returns a success bool); a failed commit is terminal for the
  process lifetime (repair = app restart).
- `engine.reset()` drops the session (install command resets before the
  self-test; remove resets before deleting files so the library is not held).

### Component manager (`local/manifest.rs` + `local/download.rs` + `commands/local_embeddings.rs`)

The pinned manifest is the source of truth: per-file URLs commit-pinned to
`/resolve/<source-revision>/` (never a moving ref), exact sizes, SHA-256
pins for EVERY file (LFS oids from the pinned repo revision; the three small
non-LFS files were hashed at build time), license fields for the consent
dialog, and profile identity enforced by `parse_manifest` against
`LOCAL_PROFILE_ID` (drift is a hard error).
Install transaction: stream each file into
`<model_root>/.staging/<LOCAL_PROFILE_DIR>/` as `<name>.part` with the hash
computed in-pass, verify pins (per-file `spawn_blocking`), promote via
`promote_install` (park old install as `.staging/replaced-<ts>`, rename
staging into place, write `manifest.json`, clean leftovers; a failed promote
RENAMES the parked install back before erroring), then report. A working
install is never partially replaced by a failed transaction.
Resume: an interrupted `<name>.part` continues with `Range: bytes=<n>-` (the
local prefix is hashed first so verification covers the whole file); a
server answering 200 instead of 206 restarts cleanly. A healthy installation
short-circuits the whole install (no network) - install doubles as repair of
corrupt/partial installs via full re-download + swap. Cancellation
(`Arc<AtomicBool>` via `LocalEmbeddingsInstallState`) is checked between
chunks AND before the final rename; each chunk read is bounded by
`READ_STALL_TIMEOUT` (120 s) so a stalled body cannot hang the install.
Fast health checks: `state::probe_installation_state` is the cheap service
gate (manifest exists); `state::assess_installation` is the plan-§5 startup
check (manifest + every file at pinned size -> `Ready`; partial presence ->
`RepairRequired`; nothing -> `NotInstalled`). The status command layers
`installing` (running flag) and `unsupported` (target gate) on top -
derived, not persisted. Full SHA-256 re-verification
(`verify_installed`, async command on `spawn_blocking`) also compares the
installed manifest's profile against the active one (r1-vs-r2 detection).
Gates before the first byte: `supported_target()` (win-x64, osx-arm64,
linux-x64 - osx-x86_64 dropped: no runtime builds) and a repair-aware
free-space check (`fs4`; `required_disk_bytes(existing)` = total x (1|2) +
max(64 MiB, 10%) counting the model files PLUS the current target's runtime
archive; a failed probe skips the gate with a stderr note).
Remove/verify commands reject while an install runs; `remove_components`
(async command on `spawn_blocking`) sweeps every candidate model root -
the resolved root, the app-data fallback, and always
`{storage_root}/model` - so a storage-root move cannot orphan an
installation in either direction.
Commands emit `embedding:component` progress events
(downloading/verifying/installing/done/error with per-file + overall bytes).
Lock discipline: the storage root is resolved under a brief lock burst and
released before any network I/O.

### Runtime component (manifest `runtime` + `download::install_runtime`)

The manifest pins ONNX Runtime 1.30.0 per target (official GitHub release
archives: linux-x64/osx-arm64 `.tgz` = flate2+tar, win-x64 `.zip` = zip
crate) with URL + size + official SHA-256 + the in-archive `libPath` (the
VERSIONED regular file - the unversioned names are symlinks in Microsoft's
archives, and copying a link entry yields zero bytes; the extractor rejects
non-regular members); validation enforces the known target set, unique
targets, hex hashes, and archive-safe lib paths. The extracted library is
chmod 0755 (dlopen needs exec; `File::create` yields umask default).
`install_runtime` downloads + hash-verifies the
archive into `{runtime_root}/.staging/`, extracts ONLY the pinned library
member into `{runtime_root}/onnxruntime/<version>/` (`extract_archive_member`
rejects traversal), writes a version manifest, removes the archive, and
short-circuits when the library already exists (idempotent; doubles as
repair). Windows lock safety (findings-7 L1): a loaded DLL cannot be
deleted/overwritten (ort keeps it resident for the process lifetime), so
`remove_components` tries a plain delete and on failure RENAMES the runtime
tree to `onnxruntime.trash-<ts>` (every remove/install sweeps stale trash
dirs), and a repair that cannot replace a locked library errors with an
actionable restart hint. The install command runs model profile -> runtime component ->
engine self-test -> persist the `Enabled`/`LOCAL_PROFILE_ID`/768 capability
triple WHEN `bango_local` is the active selection (cloud triple untouched
otherwise); remove resets the engine session + the triple when local was
active. Derived (not persisted) install states remain the design: the
running flag + `assess_installation` cover every observable transition.

### Runner v2 redesign

The runner is an OUTER `tokio::task::JoinSet` (one task per article) instead of
a sequential `for` loop. Each task calls
`EmbeddingBatchSender::send_embedding_batch_parallel` (injectable trait, mirrors
`IngestLlmSender`; production senders are the backend-aware
`BackendEmbeddingBatchSender` (`runner::backend_sender(app_handle)` resolves
backend + storage root under one brief lock; `ConfiguredProvider` wraps the
orchestrator byte-identically to the pre-router direct call, `BangoLocal`
runs `LocalEngine::embed` with the Document role) - all six production call
sites (generate/regenerate, citation finder, post-summary cascade,
chunk-rebuild cascade, batch import Phase 5) use it;
tests inject a fake), then writes its rows under a brief DB lock burst.
The trait also carries the two backend-identity seams. Operation-scoped
selection (findings-7): `backend_sender` captures the backend at
construction, so a mid-run backend switch keeps the old backend for that
operation - deliberate (one run = one consistent provider; the next
operation re-resolves). The two backend-identity seams are:
`provider_id(&LlmConfig)` (the row `provider` label; local rows record
`bango_local`) and `probe_capability(Option<&LlmConfig>, Option<&str>)`
(the Unknown-status generation probe + Test Connection probe; default = the
cloud HTTP probe, local override = the offline `service::probe_local`).
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
`&self` methods) because `JoinSet::spawn` requires `'static` futures. All six
production call sites (generate/regenerate in `commands/embedding.rs`, the
post-summary cascade in `commands/summary.rs`, the chunk-rebuild cascade in
`commands/full_text.rs`, Citation Finder, batch-import Phase 5) build the
backend-aware sender via `runner::backend_sender(app_handle)`.

## Work Guidance

- Inject `Arc<dyn EmbeddingBatchSender>` into the runner so the parallel +
  cancel behavior is unit-testable without a live provider.
- Never hold the DB mutex across an `.await`; use the three-burst lock pattern.
- The `-1` chunk_index sentinel for the title+abstract row is owned by the
  storage layer (`db/embedding_repo.rs`).

## Verification

- `tests/embedding/embedding_storage_test.rs` (11)
- `tests/embedding/embedding_text_test.rs` (5)
- `tests/embedding/embedding_provider_test.rs` (22)
- `tests/embedding/embedding_director_test.rs` (11)
- `tests/embedding/embedding_backend_setting_test.rs` (5: default, round-trip,
  garbage-fallback, backup exclusion)
- `tests/embedding/embedding_service_test.rs` (5: cloud query + documents via
  mockito, local NotInstalled error, local pending-engine error, missing
  storage root reads as NotInstalled)
- `tests/embedding/embedding_component_test.rs` (18: mockito install + verify,
  bad-hash abort, idempotent repair without refetch, cancel, corruption
  detection, profile mismatch, Range/206 resume, promote rollback,
  artifact removal across candidate roots, runtime tgz/zip member-only
  extraction, traversal rejection, integrity-aware runtime skip, runtime
  size-mismatch repair + staging cleanup, dot-prefixed macOS member
  matching, runtime verify missing/corrupt/healthy)
- `tests/embedding/embedding_engine_live_test.rs` (2, both `#[ignore =
  "slow"]`): the T4/T5 live spike (pin verification, pinned runtime archive
  install + `LocalEngine.embed` with NO dylib override, direct Q4 inference
  via ort load-dynamic; requires only `BANGO_EMBED_MODEL_DIR` - the runtime
  downloads itself) and the T8 acceptance smoke `embeddinggemma_q4_
  acceptance_smoke` (self-sufficient, network: installs BOTH pinned
  components, verifies, embeds a 200-doc corpus via the production engine,
  asserts 768-dim + count + retrieval sanity, prints install/cold-call/
  throughput/warm-latency/peak-RSS observations; linux asserts a RAM lower
  bound)
- `tests/embedding/embedding_model_mismatch_test.rs` (11)
- `tests/embedding/embedding_model_override_test.rs` (6)
- `tests/embedding/embedding_recall_test.rs` (7, incl. the `f32::NEG_INFINITY`
  max-pool sentinel regression test)
- `tests/embedding/embedding_recall_multistatus_test.rs` (12)
- `tests/embedding/embedding_runner_test.rs` (12, covering the pure
  `resolve_effective_dim` + `vector_matches_dim` helpers that drive the
  runner's per-row dimension validation, plus `BackendEmbeddingBatchSender`
  routing: local gate refusal without HTTP, offline probe outcome, provider
  labels)
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
- `embedding/local/` inline (40: `paths.rs` OneDrive detection + resolution (8),
  `thread_budget.rs` (3), `prompt.rs` (3), `state.rs` (11), `manifest.rs` (9),
  `download.rs` (2), `engine.rs` (4: vector validation
  accept/reject, dylib env-override + install-integrity resolution))

204 tests total (202 excluding the two `#[ignore = "slow"]` live tests).

## Child DOX Index

No child `AGENTS.md` files. This module owns `director.rs`, `runner.rs`,
`recall.rs`, `text.rs`, `batching.rs`, `backend.rs`, `service.rs`, `local/`,
`mod.rs` with no further durable boundaries. The provider client lives in
`llm/embedding.rs` (see `llm/AGENTS.md`); the storage layer lives in
`db/embedding_repo.rs` (see `db/AGENTS.md`).
