# Citation Finder

## Purpose

Paste-prose-to-citations matching over the user's article library. Three-layer
pipeline: embedding prefilter (reuses `embedding::recall`) → token-containment
passage extraction (pure) → LLM classification (validating/opposing +
`misrepresents_source` + cosine confidence).

Two modes: **whole-block** (one embedding, one result set) and **per-statement**
(LLM splits prose into ≤5 claims; each claim is embedded + matched
independently; results grouped by claim).

One-button flow: `find_citations` is the single entry point. It runs Phase A
(readiness) → Phase B (auto-prepare embeddings if coverage <100%, reusing
`generate_embeddings_inner`) → Phase C (the search pipeline), all under one
`Arc<AtomicBool>` cancel token. Phase B is best-effort: after it runs, the
search proceeds regardless of the post-prepare coverage (there is NO 100%
gate). Coverage can legitimately plateau below 100% when some articles have
no embeddable content (empty title + empty abstract + no full-text chunks →
`expected_rows` returns zero rows → the director produces no `EmbedTask`).
The recall layer naturally excludes those articles from the candidate pool,
which is the correct outcome (they have no semantic signal).

## Ownership

Part of `src-tauri/src/`. Sibling to `embedding/` (which it reuses for the
prefilter + prepare) and `screening/` (whose `RunSyncContext` pattern inspired
`FindCitationsContext`). The Tauri command layer lives in
`commands/citation_finder.rs`.

## Local Contracts

- **Mode wire tokens are snake_case** (`whole_block` / `per_statement`): the
  `CitationFinderMode` enum carries `#[serde(rename_all = "snake_case")]`, so
  the frontend `CitationFinderMode` type MUST mirror those exact literals
  (NOT kebab-case `whole-block`/`per-statement`). The Tauri `find_citations`
  command deserializes `mode` directly with no translation layer, so a
  kebab-case token surfaces as the user-facing error
  `invalid args mode for command find_citations: unknown variant whole-block,
  expected whole_block or per_statement`. This is consistent with the rest of
  the module's wire vocabulary (`preparing_embeddings`, `embedding_query`,
  `working`/`included`/`rejected`). Conceptual prose may still read
  "whole-block" / "per-statement", but IPC payloads use the underscores.
- **Reuses `embedding::recall::recall`** (extended to multi-status
  `&[String]`) as the cosine prefilter. No reimplemented cosine + max-pool.
- **Toggle visibility gate** (cf2.md §2.1): the readiness payload now carries
  `embedding_status: String` (`"unknown"` | `"enabled"` | `"disabled"`) +
  `embedding_model: Option<String>` so the frontend can render the toggle in a
  visible-but-disabled state when the provider is known-unsupported (Anthropic,
  Z.AI), instead of silently hiding it. The previous boolean-only gate
  (`provider_supports_embeddings`) left users on unsupported providers with no
  indication the feature existed. The frontend's `citationToggleState`
  computed derives `'enabled' | 'unknown' | 'disabled' | 'hidden'` from
  `embedding_status`; the toggle is `disabled` (with a tooltip pointing to
  Settings) only when `embedding_status == 'disabled'`, and `'hidden'` only
  when readiness has not loaded or the LLM is not configured. The backend
  `find_citations` Phase A guard still uses `provider_supports_embeddings`
  (the derived bool) so the runtime behavior is unchanged.

  **Reactivity**: the frontend `chat-view.vue` watches `llmConfigStore.config`
  (deep) + re-runs `checkCitationFinderReadiness()` on every change so a
  Settings provider switch updates the toggle live (mirrors the canonical
  `useLlmConfigured()` pattern). The previous one-shot `onMounted` check went
  stale until the user navigated away and back.

  **Static-override for known-unsupported providers (authoritative)**:
  `compute_readiness` consults `llm::embedding::check_embedding_support(provider)`
  and, when the configured provider is statically known-unsupported
  (`Anthropic` / `ZAi`), the REPORTED `embedding_status` is ALWAYS overridden
  to `"disabled"` - regardless of the persisted status value. This is
  authoritative (not just a fallback for un-probed `Unknown` state) so it
  catches un-probed `Unknown`, stale `Enabled` (left over from a previous
  OpenAI session), and save-debounce timing races. The persisted
  `app_settings.embedding_status` is NOT mutated here (read-only derivation
  for the readiness payload); the probe + runner keep reading the persisted
  value directly. Pinned by the `compute_readiness_{anthropic,zai,openai}_*`
  tests in `tests/citation_finder_readiness_test.rs`, including
  `compute_readiness_anthropic_overrides_persisted_enabled` which asserts the
  override wins over a stale persisted `enabled`.

  **Provider-card debounced save** (`settings-provider-card.vue`): the
  debounced auto-save watcher tracks `provider`, `endpointUrl`, `modelName`,
  AND `apiKeyEncrypted` in addition to the 4 Parameters fields, so changing
  the provider dropdown persists to the DB within 600ms (previously it only
  persisted on Test Connection, leaving the DB stale + breaking the readiness
  override which reads the persisted provider field). Gated on `!testing` and
  `!fetchingModels` so it doesn't race with those paths' explicit saves.

  **Probe-skipping**: Phase B's probe fires ONLY when `embedding_status ==
  Unknown`. After `Test Connection` probes and sets `Enabled`, Phase B skips
  the probe and proceeds straight to embedding generation. The earlier bug
  where the probe fired on every first Citation Finder call was caused by
  `save_llm_config` unconditionally resetting the status to `Unknown` on every
  save (including parameters-only edits from the Settings auto-save watcher),
  which wiped the `Enabled` status the probe had just set. The fix
  (`commands::llm_config::embedding_relevant_changed`) gates the reset behind
  a provider/endpoint/model/api-key comparison so parameters-only saves
  preserve the status. Pinned by `embedding_probe_persist_test.rs`.

- **Model-mismatch detection** (`get_embedding_model_mismatch` command +
  `first_mismatched_model` pure helper in `commands/embedding.rs`): before
  each submit the frontend calls `get_embedding_model_mismatch`, which returns
  `Some(EmbeddingModelMismatch { currentModel, storedModel, storedRowCount })`
  when stored embeddings were generated with a different model than the
  current `embedding_model` setting. `recall` filters by the new dimensions, so
  stale-model rows are silently excluded - without this check the user gets a
  zero-results search with no explanation. The frontend pops a confirmation
  dialog (Regenerate / Continue anyway / Cancel) keyed by `storedModel` +
  tracked in `chatStore.mismatchDismissedFor` so it doesn't nag on every
  subsequent search. The "Regenerate" path calls `regenerate_embeddings`
  (scoped delete + re-embed); "Continue anyway" records the dismissal + proceeds.

  The director's per-row staleness check was ALSO extended: a stored row is
  stale when `stored_hash != input_hash || stored_model != current_model` (was
  hash-only). This makes Phase B's auto-prepare actually regenerate stale-
  model rows on the first Citation Finder run, fixing the "100% coverage /
  zero results" silent failure at its source. Pinned by
  `director_detects_model_mismatch_as_stale` in
  `tests/embedding_director_test.rs` + the 7 pure-helper cases in
  `tests/embedding_model_mismatch_test.rs`.
- **`misrepresents_source`** (`CitationLlmOutput` field): `true` = the matched
  passage is taken out of context / selectively quoted in a way that
  misrepresents the source. The `#[serde(alias = "fairlyParaphrased")]` keeps
  deserialization backward-compatible with a stale prompt template cached
  mid-rollout.
- **Lenient LLM-response parsing** (`parse_citation_outputs` in `prompt.rs`):
  the system prompt requests snake_case field names + a bare JSON array, but
  LLMs are unreliable about casing and shape, so the parser is tolerant at
  three layers. (1) **Field-name aliases**: every `CitationLlmOutput` field
  accepts both snake_case (canonical, what the prompt asks for) and camelCase
  (what some LLMs emit) via `#[serde(alias = "...")]`. The struct is
  `Deserialize`-only - it is never serialized to the frontend (the IPC-facing
  `CitationMatch`/`CitationResult` types serialize independently with their
  own `camelCase`) - so dropping the struct-level `rename_all = "camelCase"`
  has zero IPC impact. `classification` + `relevance_explanation` carry
  `#[serde(default)]` (→ empty string); `parse_classification("")` returns
  `None` and drops the entry, so a missing classification is filtered not
  fatal. (2) **Object-wrapper tolerance**: `parse_citation_outputs` accepts a
  bare JSON array OR `{...}` with one of the known wrapper keys (`results`,
  `citations`, `data`, `matches`, `items`, `output`). (3) **Per-element fault
  isolation**: each array element is deserialized independently so one
  malformed element (missing field, typo'd key) costs only that element - the
  rest of the batch survives. If zero elements parse, the first error is
  surfaced (so genuine LLM failures aren't masked as an empty result); a
  genuine `[]` returns `Ok(vec![])`. Both `run_whole_block` and
  `run_per_statement` route through this helper; the previous inline
  `serde_json::from_str::<Vec<_>>` calls were all-or-nothing and dropped
  every good result alongside a single bad one (the exact `missing field
  articleId` bug-report failure mode). Pinned by the snake_case + wrapper +
  fault-isolation tests in `tests/citation_finder_prompt_test.rs`.
- **`normalize_claim_key`** (pure, `#[must_use]`) drives the
  `(article_id, claim)` lookup in `merge_outputs`: trim + collapse internal
  whitespace + lowercase so cosmetic claim drift between the splitter and the
  classifier (whitespace, case) does not cause a cosine-score lookup miss.
  Punctuation is intentionally NOT stripped (conservative; avoids
  false-positive pairings between distinct claims that share tokens).
- **Highlighted sentences + progressive disclosure** (`prompt.rs` +
  `search.rs` + `citation-result-card.vue`): the system prompt asks the LLM
  for 1-3 EXACT verbatim `justifying_sentences` from the passage that most
  directly justify the classification. `prompt::ground_quotes` (pure,
  `#[must_use]`) filters those through a normalized-substring gate
  (lowercase + whitespace-collapse + trim) against the actual
  `matched_passage` so paraphrases / merges / inventions are DROPPED before
  display - displaying an ungrounded sentence as a quote would fabricate text
  the paper does not contain. The survivors populate
  `CitationMatch.highlighted_sentences` (snake on the Rust side, camelCase on
  the wire). The card collapses to showing only those snippets by default
  (one tinted block per sentence); a "Show full passage" toggle expands the
  full `matched_passage` with the sentences rendered inline as `<mark>`-
  styled highlights. Empty `highlighted_sentences` (LLM omitted the field OR
  none grounded) → legacy full-passage display with no toggle (graceful
  fallback, no regression). No extra LLM call: the field rides on the existing
  classification call (~30-60 extra output tokens per result). Pinned by the
  `ground_quotes_*` + `llm_output_justifying_sentences_*` tests in
  `tests/citation_finder_prompt_test.rs` + the progressive-disclosure tests
  in `src/__tests__/components/citation-result-card.test.ts`.
- **Cosine is the user-facing "match %"**, normalized from `[-1, 1]` to
  `[0, 1]`. Containment (query coverage) is internal-only (drives passage
  selection).
- **Passage gate uses containment, NOT Jaccard** (`similarity.rs`): the gate
  is `containment(query, chunk) = |query ∩ chunk| / |query|` with
  `MIN_PASSAGE_SCORE = 0.3`. The previous Jaccard gate
  (`|A ∩ B| / |A ∪ B|`, 0.05) penalized the realistic asymmetric length ratio
  (a ~12-token query against a ~300-token chunk): an EXACT quote scored
  Jaccard ≈ 0.04 (< 0.05) and was silently dropped before the LLM ever saw
  it. Containment is length-insensitive on the chunk side (exact quote → 1.0
  regardless of chunk length), which is the standard IR metric for "short
  query, long document." `jaccard_similarity` is retained as a `pub` helper
  but is NOT the gate. Pinned by `containment_exact_quote_in_long_chunk_is_one`
  + `jaccard_diluted_by_long_chunk_exact_quote` in
  `tests/citation_finder_similarity_test.rs`.
- **`ArticleBest::cosine` seeds at `f32::NEG_INFINITY`** (NOT `Default =
  0.0`), mirroring `embedding::recall::recall`'s own max-pool, so a hit with a
  negative cosine is recorded as the article's best score instead of being
  silently discarded by a `> 0.0` guard. Without this a true negative cosine
  would surface as `0.5` (neutral) instead of `0.0` (opposite direction).
- **`Chunk.section: Option<String>`** - handled as `Option`; `None` for
  `SectionKind::Text`-derived chunks; the UI omits the `§…` badge. Abstract-
  only articles synthesize a chunk with `section: Some("Abstract")`.
- **Metadata reaches the prompt**: `format_candidates_section` renders
  title/authors/year/journal/DOI per candidate so the LLM can write informed
  relevance explanations (not classify passages blind to which article they
  came from).
- **One passage-building path**: both `run_whole_block` and `run_per_statement`
  build `CandidatePassage` lists inline next to their `load_metadata` call, so
  the two paths cannot diverge.
- **`CitationLlmSender` trait** (injectable, mirrors `EmbeddingBatchSender`):
  production `HttpCitationLlmSender` wraps `Arc<LlmOrchestrator>` +
  `AppHandle`; tests inject a fake.
- **`FindCitationsContext`** bundles `text + mode + status_filter +
  cancel_token + emit_progress + app_handle` so `find_citations_inner` stays
  under the clippy `too_many_arguments` threshold (mirrors screening's
  `RunSyncContext`).
- **Cancel token is `Arc<AtomicBool>`** (not `Mutex<bool>`), matching the
  embedding runner's contract - `generate_embeddings_inner` takes
  `Option<Arc<AtomicBool>>` and the same token covers Phase B + Phase C.
  Cancel is checked between phases, between recall/passage calls, and before
  the 120s classification call. An in-flight LLM call completes naturally (or
  hits the orchestrator timeout) before the cancel fires at the next check
  point; the frontend `cancelling` spinner covers that wait window. (The
  screening engine's millisecond `tokio::select!` cancel is not warranted for
  a single user-initiated search.)
- **Per-article lock discipline**: `build_claim_work` and `load_metadata` are
  `async` and each takes a brief `lock_conn` burst per article (releasing
  between articles, with `tokio::task::yield_now()`), so the `DbState` mutex
  is never held across the up-to-150 chunk reads (30 candidates × 5 claims)
  or the ≤15 metadata reads. Avoids the mutex-starvation anti-pattern
  `db/AGENTS.md` flags.
- **Clean error messages**: the `citation:error` event strips the
  `AppError::Import` `"Import error: "` prefix (`raw.strip_prefix(...)` in
  `commands/citation_finder.rs`) so the frontend receives bare user-facing
  messages ("Cancelled", "Provider does not support embeddings…").
- **`overall_percent` is computed**: `phase_b_overall_percent(done, total)`
  maps Phase B to the 0-90% range; `phase_c_overall_percent(stage)` returns
  fixed offsets (embed_query=90, ranking=93, classifying=96, done=100). The
  frontend's `embedding:progress` listener translates each runner payload
  into the same 0-90% range.
- **Status whitelist + no backend default**: `filter_valid_statuses` (pure,
  `#[must_use]`, in `mod.rs`) filters the caller's `status_filter` against
  `["working","included","rejected"]` at the `find_citations_inner` boundary.
  `duplicate` is always dropped. An empty post-filter list returns the "No
  articles match the selected filters." empty result - the backend never
  falls back to "all statuses" (the standalone `recall_articles` command's
  empty-means-all contract is a separate path).
- **No new `LlmRequestType::skip_temperature` participation** -
  `CitationFinder` / `CitationFinderSplit` are standard chat-completion calls
  routed through `send_json` (JSON pre-parser applied).
- **Timeouts**: `CitationFinder` = 120s (15 candidates + passages);
  `CitationFinderSplit` = 60s (small prompt but local LLM cold-start).

## Work Guidance

Follow `docs/CLAUDE.md` (Rust error handling with `?` + `AppError`, no
`unwrap()` in production). The `#![cfg_attr(not(test), warn(clippy::unwrap_used,
clippy::expect_used, clippy::panic))]` gate in `lib.rs` applies. Pure helpers
(`similarity.rs`, `prompt.rs`, `claim_splitter.rs`, `readiness.rs`'s
`coverage_percentage`) are `#[must_use]` + unit-tested.

Per `docs/CLAUDE.md` §Testing, pure-helper unit tests live in external
`src-tauri/tests/citation_finder_*_test.rs` files. The `search.rs` pipeline
tests stay inline because they exercise private internals (`merge_outputs`,
`pool_finalists`, `ClaimWork`, `Finalists`) that cannot be reached from an
external test.

## Verification

- `cargo test --lib citation_finder` - the inline `search.rs` tests
  (`normalize_claim_key`, `merge_outputs` whole-block + per-statement +
  drift-tolerance + drop paths + cosine-normalization edge cases,
  `pool_finalists` dedup/truncate/empty).
- `cargo test --test citation_finder_similarity_test` - containment +
  Jaccard (failure-mode pin) + `find_best_passage` + tokenizer edge cases.
- `cargo test --test citation_finder_prompt_test` - system-prompt shape,
  whole-block/per-statement structure, metadata render, `parse_classification`,
  `CitationLlmOutput` deserialization (camelCase alias + snake_case canonical
  + mixed-case), and `parse_citation_outputs` lenient parsing (bare array,
  object wrappers, per-element fault isolation, bug-report regression).
- `cargo test --test citation_finder_claim_split_test` - `enforce_max_claims`
  truncation/trim/drop + prompt builder.
- `cargo test --test citation_finder_readiness_test` - `coverage_percentage`
  edge cases.
- `cargo test --test citation_finder_mod_test` - `filter_valid_statuses`
  whitelist contract.
- `cargo test --test citation_finder_search_test` - the public
  `normalize_claim_key` pipeline contract (external pin).
- `cargo test --test embedding_recall_multistatus_test` - the multi-status
  prefilter extension.
- `cargo clippy --lib -- -D warnings` - clean (the project gate).
- `cargo fmt --check` - clean.
- Frontend: `npx vitest run src/__tests__/composables/use-citation-finder.test.ts
  src/__tests__/components/citation-result-card.test.ts src/__tests__/chat.test.ts`
  - formatCitation/findCitations + card + store (incl. citation-finder).

## Child DOX Index

No child docs. The module is a flat set of 5 files (`mod.rs`, `search.rs`,
`similarity.rs`, `prompt.rs`, `claim_splitter.rs`, `readiness.rs`) + the
command shim in `commands/citation_finder.rs`.