# wiki/

## Purpose

LLM Wiki knowledge-base module (all phases complete). Generates and maintains
a local-first Obsidian-style Markdown knowledge base from the `included`
article corpus.

## Ownership

- Owns: `storage.rs`, `agents_contract.rs`, `templates.rs`, `frontmatter.rs`,
  `raw_export.rs`, `fts.rs`, `engine.rs`, `chat.rs`, `ingest/` (directory
  module), `mod.rs`.
- Commands live in `commands/wiki_cmd/` (directory module).
- Frontend: `wiki-view.vue`, `wiki-toolbar.vue`, `wiki-page-viewer.vue`,
  `wiki-page-editor.vue`, `wiki-graph-panel.vue`; composable `use-wiki.ts`;
  types `types/wiki.ts`; markdown renderer `utils/wiki-markdown.ts`;
  static-site exporter `utils/wiki-site-export.ts`.

## Local Contracts

### Modules

- `storage.rs` - resolves `wiki-root/`, scaffolds `raw/`,
  `wiki/{concepts,authors,methods,synthesis}/`, `templates/`, `AGENTS.md`,
  `log.md`.
- `agents_contract.rs` - ingest + lint rules contract.
- `templates.rs` - page templates.
- `frontmatter.rs` - dependency-free YAML parser/serializer.
- `raw_export.rs` - included-article export + user-file extraction for
  PDF/TXT/HTML/etc. `resolve_user_file_title` enriches PDF titles via `lopdf`
  (reads the `/Title` entry from the Info dictionary) so the pre-seed source
  page + LLM prompt use the document's real title instead of the filename stem.
  `prepare_all_with_progress` splits the article load (under DB lock) from the
  file writes (lock-free) so the CPU-bound `structure_full_text` extraction does
  not block other IPC commands.
- `fts.rs` (T1.2 update) - chunk-aware FTS5 schema: `ensure_table` creates
  `chunk_index UNINDEXED, section UNINDEXED, parent_slug UNINDEXED` columns.
  `PageRow` carries the same three optional fields; `WikiPageHit` surfaces them.
  `ensure_index_populated` self-heal compares
  `COUNT(DISTINCT COALESCE(parent_slug, slug))` against disk page count (not raw
  row count) so chunk rows do not false-positive a rebuild on every chat call.
  `strip_table_placeholders` is `pub` so the integration test can exercise it
  directly.
- `ingest/` (directory module: LLM page generation - prompt builder,
  `<!-- PAGE:slug -->` response parser, page writer, FTS5 rebuild, parallel
  chunked ingest; submodules: `mod.rs` core pipeline + re-exports,
  `batching.rs`, `consolidation.rs`, `authors.rs`, `synthesis.rs`,
  `concepts.rs`, `sources.rs`, `slugs.rs`). Inline tests extracted to
  `tests/wiki_ingest_test.rs` per `docs/CLAUDE.md` §Testing.
- `engine.rs` - deterministic lint + `build_graph` for link graph
  visualization. `LintKind::UngroundedPage` (ERROR-level provenance check).
- `chat.rs` (T1.2 update) - token-budgeted RAG chat over FTS5 index; self-heals
  the FTS table via `fts::ensure_index_populated` when the index is empty OR its
  row count mismatches the number of `.md` pages on disk. `MAX_HITS` raised
  from 8 to 16. `build_context` dedupes by `parent_slug` (keeps top-ranked
  chunk per page, appends "(+N more passages from this page)"). `format_entry`
  includes `(§Methods)` in the header when `hit.section` is present.

### Parallel chunked ingest (`ingest/batching.rs`)

`wiki_ingest`, `wiki_rebuild`, and `wiki_export_and_ingest` no longer make one
monolithic LLM call. They split raw sources into batches sized to
`config.context_window_tokens * 0.4` (input budget; remainder is available for
output pages), dispatch all batches concurrently via a
`tokio::task::JoinSet` (bounded by the orchestrator's
`max_concurrent_requests` semaphore), and emit `wiki:progress` on every batch
completion so the progress bar moves smoothly across the 25-95% range. Each
batch carries a compact full-source index (title + slug) so the model can
`[[link]]` across batches without sequential slug-forwarding. Per-batch
failures are tolerated (recorded in `report.errors`; other batches still
write). Key types: `IngestBatch`, `IngestLlmSender` (injectable trait;
production `OrchestratorIngestSender`, test `FakeSender`),
`build_ingest_prompt_batches`, `run_chunked_ingest`. (`write_pages_from_response`
remains for the async write-and-index path; the legacy single-call
`build_ingest_prompt` was deleted - the batch path now covers all production
callers.)

### Multi-batch consolidation (gated on `batches.len() > 1`)

When the corpus splits into multiple parallel batches, independent batches
often produce near-duplicate pages for the same concept (`childhood-obesity`
vs `obesity-childhood`). To prevent fragmentation, `run_chunked_ingest`
collects all `ParsedPage`s across batches, runs a **deterministic**
`consolidate_pages` pass (no LLM merge calls), rewrites inbound `[[wikilinks]]`
to canonical slugs via `rewrite_page_links`, then writes the consolidated set.
Detection: two same-type (non-author) pages merge when (a) slugs match
case-insensitively, OR (b) stemmed-token Jaccard similarity of slugs >=
`DEDUP_JACCARD_THRESHOLD` (0.5), OR (c) they share >=
`DEDUP_SHARED_SOURCES_MIN` (2) `source_articles`. Merge is lossless: the
duplicate body is appended under `## Additional perspectives`; `source_articles`
+ `tags` are unioned. Author pages are pre-seeded and excluded from merging.
`AuthorManifest` + `preseed_authors` + `build_author_manifest` derive canonical
author slugs from `biblio_authors` (populated by running
`run_full_normalization` first - the full 8-step bibliometric pipeline
extracted into a pure `pub fn run_full_normalization(conn)` in
`biblio_repo/normalization.rs` and shared by both `biblio_normalize` and the
wiki ingest path, so there is no raw-frontmatter fallback) and inject a "DO NOT
create author pages" section into every batch prompt so batches link to the
same author slugs instead of inventing their own. Each pre-seeded author page
is a rich hub: metrics line (h-index, total citations, first-author count,
papers/year), Publications list with `[^art-id]` footnotes + real
`source_articles` frontmatter, Research Areas (deduplicated keywords
aggregated from `biblio_article_terms`), and Frequent Collaborators
(`[[author-slug]]` links derived from shared-paper counts).

Single-batch runs (`batches.len() == 1`) skip all consolidation - the LLM sees
all sources at once and produces a self-consistent page set, so the manifest,
pre-seed, dedup, and link rewrite are zero-cost no-ops.

### Cancel-token + progress contract (v2, see `.worktrees/wiki2.md`)

All three entry points (`wiki_ingest`, `wiki_rebuild`,
`wiki_export_and_ingest`) snapshot a fresh `Arc<AtomicBool>` into the managed
`WikiIngestState` (`commands/wiki_cmd/mod.rs`, mirrors `ScrapingState`) at
start and clear it on return. The frontend `cancel_wiki_ingest` command signals
the active token. The pipeline checks `is_cancelled` between each of the 7
pre-seed steps in `build_batches_with_manifest` (on cancel: `Ok(Vec::new())` =
empty batches = no LLM calls) and between `join_next().await` completions in
`run_chunked_ingest` (on cancel: `join_set.abort_all()`, drop in-flight
results, return `Ok(report)` with `report.errors.push("Cancelled")`). There is
no `Cancelled` error variant - mirrors the screening engine's
`Ok(true)`/`Ok(false)` convention. The `WikiPrepProgressCb` callback fires at
each pre-seed step with `(step_pct, message)` in the 15-25% range so the
frontend progress bar advances past 15% with a meaningful phase label instead
of freezing silently. The `biblio_needs_refresh` flag gates
`run_full_normalization` (skip when fresh = the common case after visiting the
Bibliometrics dashboard). `[wiki:diag]` always-on logging (mirrors
`[screening:diag]`) emits phase transitions + cancel detection to stderr.

### Deterministic 5-layer pre-seed matrix (`build_batches_with_manifest` in `commands/wiki_cmd/ingest.rs`)

Runs unconditionally before the LLM on every single-batch AND multi-batch run:

1. `preseed_authors` writes rich author pages from `biblio_authors` (metrics,
   publications, research areas, collaborators).
2. `preseed_synthesis_from_ai_summaries` writes one
   `wiki/synthesis/{article_id}.md` per included article that has a
   `full_text_ai_summary` JSON blob - slug = article UUID (so `[[uuid]]` links
   resolve), body = `summary_150_250_words` digest + `key_insights` bullets,
   `tags` = keyword-derived `[[concept-slug]]` candidates.
3. `preseed_concept_hubs` writes `wiki/concepts/{slug}.md` hub pages from
   **two sources, slug-merged so tags win on collisions**: (a) top-40
   user-curated tags by included-article count (the highest-signal source;
   multi-word domain concepts like `supply-chain-management` that the
   unigram-only `biblio_terms` extraction cannot produce; display name via
   the pure `tag_to_display_name` helper), then (b) top-25 `biblio_terms` by
   frequency (backfill for concepts the user hasn't tagged). When a tag and a
   term normalize to the same slug, the term's articles + co-occurring
   concepts are UNIONED into the tag's page (lossless). `fetch_top_tags` +
   `fetch_top_terms` are separate fns so `methods::fetch_methods_from_terms`
   (the abstracts-only fallback) still calls the terms-only path unchanged.
   Each concept page links to its articles (`[[uuid]]`) + co-occurring
   concepts. Tested in `tests/wiki_concepts_tags_test.rs` (11 tests).
4. `preseed_methods` writes top-25 `wiki/methods/{method-slug}.md` hub pages
   from AI-summary `study_design` (when present) with a `biblio_terms`
   fallback for abstracts-only corpora; a curated study-design lexicon
   (`STUDY_DESIGN_LEXICON` in `ingest/methods.rs`) canonicalizes synonyms
   (e.g. "RCT" → `randomized-controlled-trial`) so non-methodological terms
   are filtered. When the pre-seed writes >=1 method page, the batch directive
   tells the LLM methods are handled (link, don't duplicate); when it writes 0
   pages, the directive flips to "methods NOT pre-seeded - create them" + the
   focus list always asks the LLM for METHOD pages so `wiki/methods/` is never
   empty.
5. `preseed_document_source_pages` writes one
   `wiki/sources/{user-slug}.md` per user-uploaded document (Add Documents →
   PDF/TXT/web, identified by `source_kind: user_*`) so external documents get
   a first-class wiki node and `[^art-user-slug]` / `[[user-slug]]` citations
   resolve to a navigable page instead of "Page not found". This layer mirrors
   the article→synthesis symmetry: every raw source has a corresponding wiki
   node.

All five respect `status: reviewed` (user-edited) pages. Together they form a
connected graph backbone (author ↔ synthesis ↔ concept ↔ method ↔ source) that
exists before the LLM runs, so the wiki is never missing
author/synthesis/concept/method/source pages regardless of which LLM model is
used. Tested in `wiki_deterministic_test.rs` + `wiki_methods_preseed_test.rs`.

### No title in body contract

The 5 pre-seed renderers (`render_concept_hub`, `render_author_page`,
`render_method_hub`, `render_synthesis_page`, `render_document_source_page`) +
the 5 seed templates (`templates.rs`) + the LLM batch prompt (`batching.rs`
"Do NOT start the Markdown body with a `# <Title>` heading" instruction) + the
wiki `AGENTS.md` contract (`agents_contract.rs` `## Rules` section) ALL omit
the `# {title}` heading from the Markdown body. The page title lives in the
`title:` frontmatter field and is rendered separately by the wiki viewer's
header (`wiki-page-viewer.vue` `<h1>{{ page.title }}</h1>`); repeating it in
the body would display the title twice on the rendered page. The static-site
exporter (`wiki-site-export.ts::wrapPageHtml`) emits its own `<h1>{title}</h1>`
so exported pages still have a visible heading now that the body no longer
carries one. Existing user-edited pages (`status: reviewed`) are preserved by
the pre-seed; only draft pages get regenerated on the next wiki ingest.

### Tier A1 grounding gate (`engine.rs` `LintKind::UngroundedPage`)

After every ingest, `run_chunked_ingest` runs `engine::lint` and counts pages
failing the ERROR-level provenance check (LLM-generated
concept/method/synthesis pages missing `source_articles` frontmatter).
Author/source pages are exempt (pre-seeded with a different provenance shape).
The WARNING-level check (missing `[^art-]` citations in the body) surfaces via
the standalone `wiki_lint` command. The error count is appended to
`IngestReport.errors` so the UI + diagnostics can flag ungrounded pages. Tested
in `wiki_grounding_test.rs`.

### Temperature inheritance

Wiki ingest inherits the global `LlmConfig.temperature` (default 0.2, suitable
for deterministic KB generation). There is no per-`LlmRequestType` override;
users targeting maximal determinism should set it to `0` in Settings and rely
on `skip_temperature` for incompatible models (the orchestrator + `client.rs`
own the `skip_temperature` gate + retry-without-temperature path; see
`llm/AGENTS.md`).

### Commands (`commands/wiki_cmd/`)

Directory module since refactor v6: `mod.rs` + `status.rs`, `raw_files.rs`,
`pages.rs`, `search_lint.rs`, `chat.rs`, `ingest.rs`, `site_export.rs`; `pub
use` re-exports in `mod.rs` keep all `crate::commands::wiki_cmd::*` import
paths identical, and glob re-exports surface the `#[tauri::command]`
macro-generated `__tauri_command_name_*` consts the `lib.rs` `invoke_handler!`
references. Exposes all Tauri commands: `wiki_get_status`, `wiki_init`,
`wiki_export_raw`, `wiki_add_raw_file`, `wiki_list_raw_files`, `wiki_search`,
`wiki_lint`, `wiki_get_page`, `wiki_update_page`, `wiki_delete_page`,
`wiki_delete_wiki`, `wiki_chat`, `wiki_get_graph`, `wiki_ingest`,
`wiki_list_pages`, `wiki_list_sources`, `wiki_rebuild` (one-click full
pipeline: scaffold + export + ingest + FTS5, emits `wiki:progress` events),
`wiki_export_and_ingest` (export + ingest after Add Documents), and
`wiki_check_for_updates`, plus `wiki_export_site` (static-site zip export: the
frontend renders all HTML via `renderWikiMarkdown(staticMode)` + depth-aware
`slugToHref`/`artIdToHref` resolvers and passes a `SiteExportBundle` to this
command, which writes the staging dir, copies the wiki + user-doc Markdown
tree, zips, and moves the zip to the frontend-chosen path; no
`blocking_pick_file` in the backend).

`wiki_search` rebuilds the FTS index if empty; `wiki_update_page` /
`wiki_delete_page` rebuild it on every edit/delete so chat + search stay in
sync with user changes (both use `rebuild_index_with_manifest` so the drift
manifest stays in sync too).

### `wiki_delete_wiki` de-initializes the wiki

Not just clears the `wiki/` subtree: removes `AGENTS.md` too so
`status.initialized` becomes `false` and the wiki-view shows the "Initialize
Your Wiki" empty-state card after deletion. Keeps `raw/` and `templates/` so
source documents survive for a future rebuild. The self-healing
`ensure_initialized` guard re-creates `AGENTS.md` when the user clicks any
ingest action. Also clears `wiki_needs_refresh` (defense-in-depth). Tested in
`tests/wiki_test.rs::delete_wiki_de_initializes_by_removing_agents_md` +
`delete_then_mark_staleness_does_not_re_initialize`.

### External-edit drift detection (`wiki_check_for_updates`, async)

Detects when external programs edit `wiki/**/*.md` files and re-indexes them
transparently WITHOUT re-running the LLM ingest. Runs entirely on the tokio
runtime - all file reads + per-file SHA-256 hashing happen lock-free; the
`DbState` mutex is held only for millisecond-scale SQLite writes (FTS5 rebuild
+ manifest rewrite + dir-hash update). Two tiers keep the common case cheap:
tier-1 is a stat-only directory fingerprint (`wiki_dir_hash` in
`app_settings`) that short-circuits when nothing changed; tier-2 is the
`wiki_index_manifest` table (per-file content hashes) that distinguishes real
edits from `touch`. Triggers: Wiki view `onMounted`, Chat view `onMounted`
(when wiki-ready), and the toolbar "Check for Updates" button (manual,
bypasses the 30s debounce in `use-wiki.ts`). Emits `wiki:files-changed` on
rebuild. Toast UX: "Checking for Wiki updates..." -> "Wiki updated: N pages
re-indexed." / "Wiki is up to date."

### Self-healing init guard

`ensure_initialized(root)` writes `AGENTS.md` when missing; called at the top
of `wiki_init`, `wiki_ingest`, `wiki_rebuild`, and `wiki_export_and_ingest` so
an uninitialized wiki transparently recovers instead of leaving generated
pages invisible behind the wiki-view "Initialize" empty-state gate
(`initialized` is `AGENTS.md`-presence-based). Idempotent: never overwrites an
existing `AGENTS.md`. Tested in `wiki_ensure_initialized_test.rs`.

### Staleness flag

The `wiki_needs_refresh` flag triple lives in `db/app_settings_repo.rs`;
cleared after `wiki_ingest`/`wiki_rebuild` commits. See `db/AGENTS.md` for the
full flag contract.

## Work Guidance

- Design and phasing: `.worktrees/DONOTUSE/llmwiki-plan.md`; external-document
  ingestion + linking design in `.worktrees/DONOTUSE/wiki-improvement-plan.md`
  + `.worktrees/DONOTUSE/wiki-improvement-plan2.md`; hallucination-reduction
  plan (methods pre-seed + grounding gate + prompt cleanup) in
  `.worktrees/wiki-implementation.md`.

## Verification

- `tests/wiki_fts_test.rs` (36 unit tests)
- `tests/wiki_ingest_test.rs` (6 freeze tests)
- `tests/wiki_consolidation_test.rs` (multi-batch consolidation)
- `tests/wiki_index_drift_test.rs` (two-tier external-edit drift detection)
- `tests/wiki_concepts_tags_test.rs` (11 tests)
- `tests/wiki_deterministic_test.rs` + `tests/wiki_methods_preseed_test.rs`
  (5-layer pre-seed)
- `tests/wiki_grounding_test.rs` (Tier A1 grounding gate)
- `tests/wiki_ensure_initialized_test.rs` (self-healing init guard)
- `tests/wiki_test.rs` (de-init on delete + staleness)
- `tests/wiki_full_text_refresh_test.rs` (staleness pairing)
- `src/__tests__/composables/use-wiki.test.ts` +
  `src/__tests__/components/wiki-toolbar.test.ts` +
  `src/__tests__/views/wiki-view.test.ts`
- inventory in `docs/test-plans/wiki-ingest-freeze-tests.md`

## Child DOX Index

- **`ingest/`** - LLM page generation (batching, consolidation, authors,
  synthesis, concepts, sources, methods, slugs). No own `AGENTS.md` yet; the
  contracts above cover the ingest pipeline.