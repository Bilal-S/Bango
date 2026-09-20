# wiki/

## Purpose

LLM Wiki knowledge-base module (all phases complete). Generates and maintains
a local-first Obsidian-style Markdown knowledge base from the `included`
article corpus.

## Ownership

- Owns: `storage.rs`, `agents_contract.rs`, `templates.rs`, `frontmatter.rs`,
  `raw_export.rs`, `fts.rs`, `engine.rs`, `chat.rs`, `obsidian_export.rs`,
  `ingest/` (directory module), `mod.rs`.
- Commands live in `commands/wiki_cmd/` (directory module).
- Frontend: `wiki-view.vue`, `wiki-toolbar.vue`, `wiki-page-viewer.vue`,
  `wiki-page-editor.vue`, `wiki-graph-panel.vue`; composable `use-wiki.ts`;
  types `types/wiki.ts`; markdown renderer `utils/wiki-markdown.ts`;
  static-site exporter `utils/wiki-site-export.ts`.

## Local Contracts

### Modules

- `storage.rs` - resolves `wiki-root/`, scaffolds `raw/`,
  `wiki/{concepts,authors,methods,frameworks,synthesis}/`, `templates/`, `AGENTS.md`,
  `log.md`.
- `agents_contract.rs` - ingest + lint rules contract (includes the
  Theoretical Frameworks isolation + publication-linking rules).
- `templates.rs` - page templates (concept, method, framework, synthesis,
  author, source).
- `frontmatter.rs` - dependency-free YAML parser/serializer.
- `raw_export.rs` - included-article export + user-file extraction for
  PDF/TXT/HTML/etc. `resolve_user_file_title` enriches PDF titles via `lopdf`
  (reads the `/Title` entry from the Info dictionary) so the pre-seed source
  page + LLM prompt use the document's real title instead of the filename stem.
  `prepare_all_with_progress` splits the article load (under DB lock) from the
  file writes (lock-free) so neither blocks other IPC commands.
  Content resolution (wikifix-final Change 1): `article_content` renders the
  full unified AI-summary blob as structured Markdown (summary, key insights,
  keywords, field, section summaries + typed facts; no word-count caps), falls
  back to legacy plain-text blobs, then `abstract_text`. Article full text is
  NEVER exported (`structure_full_text` was removed with the full-text branch).
- `fts.rs` (T1.2 update) - chunk-aware FTS5 schema: `ensure_table` creates
  `chunk_index UNINDEXED, section UNINDEXED, parent_slug UNINDEXED` columns.
  `PageRow` carries the same three optional fields; `WikiPageHit` surfaces them.
  `ensure_index_populated` self-heal compares
  `COUNT(DISTINCT COALESCE(parent_slug, slug))` against disk page count (not raw
  row count) so chunk rows do not false-positive a rebuild on every chat call.
  `strip_table_placeholders` is `pub` so the integration test can exercise it
  directly.
- `obsidian_export.rs` - Obsidian vault export pre-processing (pure, no DB /
  Tauri state; the command layer in `commands/wiki_cmd/obsidian_export.rs`
  loads article rows and calls in). `build_article_slug_map` renames article
  (synthesis) pages to `{surname}{year}{title-word}` slugs via the shared
  BibTeX helpers (`bibtex::writer::citation_key_from_parts` +
  `dedupe_keys`; letter-suffix dedup, `anon`/`nd` fallbacks, ASCII folding)
  with aliases `Surname et al. Year`. `rewrite_frontmatter` maps
  `id`/`slug`/`source_articles`/`links` UUIDs to slugs (unmapped UUIDs fall
  back to the page stem or are dropped) and strips `source_file` /
  `source_hash`. `rewrite_body` maps `[[uuid]]` wikilinks (alias kept for
  unmapped), `[^art-<uuid>]` refs + `/raw/<uuid>.md` definitions (rewritten
  to `[[slug|Alias]]` wikilinks; missing definition blocks appended), and
  bare `/raw/<uuid>.md` paths; unmapped UUIDs are dropped so the vault is
  UUID-free. `write_vault` stages to `wiki-root/obsidian-export/` (cleared
  each run, outside `wiki/` so FTS + drift detection are untouched):
  renames synthesis files (cross-type filename collision guard, orphaned
  pages fall back to their own frontmatter title or are skipped + counted in
  `VaultStats.skipped_orphans`), skips `log.md` + `index.md`, writes a
  generated `Home.md` + minimal `.obsidian/` config (`app.json` +
  `graph.json` color groups per folder mirroring `wiki-graph-panel.vue`).
  Frontend entry points (parity with "Export Website"): the
  `wiki-toolbar.vue` Actions menu item (gated on `isInitialized()`) and the
  shared export dialog button, both via `useExport().exportObsidian()`
  (save dialog, zip filter, `wiki_export_obsidian`).
- `ingest/` (directory module: LLM page generation - prompt builder,
  `<!-- PAGE:slug -->` response parser, page writer, FTS5 rebuild, parallel
  chunked ingest; submodules: `mod.rs` core pipeline + re-exports,
  `batching.rs`, `consolidation.rs`, `authors.rs`, `synthesis.rs`,
  `concepts.rs`, `sources.rs`, `slugs.rs`). Inline tests extracted to
  `tests/wiki/wiki_ingest_test.rs` per `docs/CLAUDE.md` §Testing.
  Batching budget (wikifix-final Change 2): `batch_input_char_budget` =
  `window * 0.4 * 4` chars, clamped `[4_000, 2_000_000]`
  (`MAX_BATCH_INPUT_CHARS = 2_000_000`, about 500K input tokens; the old 80K
  cap budgeted every window >= 50K tokens identically).
- `engine.rs` - deterministic lint + `build_graph` for link graph
  visualization. `LintKind::UngroundedPage` (ERROR-level provenance check;
  grounded types: concept, method, framework, synthesis).
- `chat.rs` (T1.2 update) - token-budgeted RAG chat over FTS5 index; self-heals
  the FTS table via `fts::ensure_index_populated` when the index is empty OR its
  row count mismatches the number of `.md` pages on disk. `MAX_HITS` raised
  from 8 to 16. `build_context` dedupes by `parent_slug` (keeps top-ranked
  chunk per page, appends "(+N more passages from this page)"). `format_entry`
  includes `(§Methods)` in the header when `hit.section` is present.

### Theoretical Frameworks page type (`framework`)

`type: framework` pages isolate named theories/models/lenses under
`wiki/frameworks/` (scaffolded by `storage::SUBDIRS`, routed by
`ingest::write_page`). They are LLM-created via the batch-prompt focus list
and the `framework.md` template - NOT deterministically pre-seeded. Contract:
every framework page ends with a `## Publications Using This Framework`
section listing one `[[article-id|Author et al. Year]]` wikilink per applying
article (alias form so Obsidian shows readable citations), `source_articles`
frontmatter mirrors those ids, and the Tier A1 grounding gate treats
`framework` as a grounded type. Frontend: sidebar + static-site label
"Theoretical Frameworks", graph legend label "Frameworks" (compact for the
legend box), color teal `#14b8a6`.

### Parallel chunked ingest (`ingest/batching.rs`)

`wiki_ingest`, `wiki_rebuild`, and `wiki_export_and_ingest` no longer make one
monolithic LLM call. They split raw sources into batches sized to BOTH the
input budget (`config.context_window_tokens * 0.4`, clamped
`[4_000, 2_000_000]` chars) and the estimated output budget
(`client::estimated_output_budget_tokens` * 0.7 / ~2.2K tokens per article,
adaptive 3-7 call sweet spot; `estimated_output_tokens = 0` disables the
output side - the legacy `build_ingest_prompt_batches` wrapper), dispatch all
batches concurrently via a `tokio::task::JoinSet` (bounded by the
orchestrator's `max_concurrent_requests` semaphore), and emit `wiki:progress`
on every batch completion so the progress bar moves smoothly across the
25-95% range. Each batch carries a compact full-source index (title + slug)
for cross-batch linking, an Existing Pages Index (slug/type/title of every
page under `wiki/`; Change 6.1 page-vocabulary pinning: reuse existing slugs,
fork nothing), and qualitative page guidance ("a page for every distinct
theme, method, or framework; typically yields several pages; no padding" -
no numeric quotas per the Tier D1 anti-hallucination rule, no word caps). A
lone
oversize source is word-boundary truncated with a disclosure marker and
counted in `report.source_chars_truncated` (Change 3). Per batch,
`process_batch` parses the response, drops a partial trailing page when the
provider reports truncation (`finish_reason`/`stop_reason` via
`IngestLlmSender::send_with_truncation`) or the trailing block fails to parse
(structural), re-dispatches bounded continuations (max 2) carrying only
sources missing from the pages' `source_articles`, and reports uncovered
sources as non-fatal errors; a 0-page parse is never silent. `run_chunked_ingest`
records run metrics (pages per type, chars, truncation/continuation counts) in
`wiki-root/.ingest-metrics.json` and warns in `report.warnings` when LLM pages
drop >20% vs the previous run (Change 6.2). Per-batch call failures are
tolerated (recorded in `report.errors`; other batches still write). Key types:
`IngestBatch` (carries `PromptContext` + sources for continuation rendering),
`IngestLlmSender` (injectable trait; production `OrchestratorIngestSender`,
test `FakeSender`), `build_ingest_prompt_batches_with_budgets`,
`run_chunked_ingest`, `IngestRunMetrics`. (`write_pages_from_response`
remains for the async write-and-index path; the legacy single-call
`build_ingest_prompt` was deleted - the batch path now covers all production
callers.)

### Local (Bango AI) ingest bounds + progress ticker (v9)

A live local run produced a single `WikiIngest` call that generated 26K+ tokens with no EOS
(the per-call timeout was the only bound), so every local generation is now capped and local
batching is sized for the cap:

- `llm::orchestrator::local_max_tokens_for(request_type)` is sent as `max_tokens` on the
  `BangoAi` path only (WikiIngest 8192, corpus reports/CitationFinder 4096, chat 4096,
  summary types 3072, classification/structured 2048; embeddings uncapped). Prose calls
  with the reasoning toggle ON get 2x headroom because thinking tokens count against the cap.
  A cap hit surfaces via `CallMeta::truncated_by_output_budget()` and flows through the
  existing truncation handling (drop partial page + bounded continuation).
- `wiki_batch_output_budget` (pure, `ingest/batching.rs`) floors the estimated output budget
  to the local WikiIngest cap: 8192 -> `max_articles_per_batch = 2`, so local batches hold
  two sources and share one prompt prefix instead of the earlier 4096 singleton batches.
  `OrchestratorIngestSender::max_continuations()` returns
  `LOCAL_MAX_CONTINUATIONS_PER_BATCH = 1` on Bango AI (cloud keeps 2).
- `dedupe_pages_by_slug` (parser-level repetition guard) collapses repeated slugs per response
  and across continuations, keeping the last body at the first position; the batch prompt also
  ends with an explicit "stop immediately, never repeat a page" instruction.
- `run_chunked_ingest_with_progress` races an `IngestProgressConfig` tick (production 5 s via
  `IngestProgressConfig::production(call_timeout)`) against `join_next()`. The tick doubles as
  the cancel poll for in-flight generations (previously cancel was only checked between batch
  completions, so a single runaway call ignored Stop) and alternates `wiki:progress` messages:
  `Generating (batch X/Y, M:SS) via LLM...` / `Estimating M:SS to go...`. The M:SS is the time
  since the last batch completion (initialized to run start); past `slow_batch_threshold`
  (`min(5 min, 40% of the active call timeout)`) the liveness half gains
  `, slow - limit M:SS` where the limit is the active per-call timeout (`1:00:00`-style
  `format_limit`: 60:00 local, 10:00 provider). **Backend-agnostic**: the ticker, ETA, and
  cancel poll run for configured-provider runs too; only `call_timeout` differs (the command
  entry points pass `resolve_timeout(&LlmRequestType::WikiIngest, is_local_config(&config))`).
  `run_chunked_ingest` is the default-config wrapper. `format_duration`, `format_limit`,
  `format_eta`, `slow_batch_threshold`, and `progress_tick_message` are pure and unit-tested;
  the cancel arm is tested on paused tokio time (virtual tick + signal, no wall-clock waits).
- A cancelled ingest returns `report.errors = ["Cancelled"]`; `finalize_ingest` then leaves
  `wiki_needs_refresh` SET so the next Update retries (only a completed run clears it).
  llama-server cancels the slot itself when the client drops the request (`stop: cancel task`
  in the engine log), so the next serialized call does not queue behind a zombie.
- `ensure_wiki_summaries` and `polish_framework_pages` are skipped when the backend is
  `bango_ai` (`[wiki:diag]` logs both): per-article full-text summaries and per-framework
  polish are the heaviest serialized local calls, and the abstract fallback plus the
  deterministic framework skeleton keep the wiki correct. The skipped-summary count is
  returned to the caller and surfaces as a report warning ("N full-text article(s) have no
  AI summary; Bango AI uses their abstracts...").

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

### Theoretical Frameworks: extraction, canonicalization, pre-seed, polish (`ingest/frameworks.rs`)

Root cause (2026-09-16): the wiki input switch to AI summaries dropped
named-theory mentions, so the LLM ingest could no longer ground framework
pages (three fresh runs produced zero). The blob now carries
`theoretical_frameworks: [{name, usage}]` (both summary prompt schemas ask
for it; an ensure-frameworks pre-phase backfills + canonicalizes it on
`wiki_ingest`/`wiki_rebuild`/`wiki_export_and_ingest`).

- Canonicalization runs before anything is generated out: deterministic slug
  clustering (`canonical_name_map`, most-frequent variant wins) + ONE
  corpus-level LLM alias-merge call (`apply_alias_merges` unifies acronym
  variants like DSM-5 vs its full title); blobs are rewritten to the
  canonical names so every consumer sees identical names.
- Deterministic skeleton pre-seed (`preseed_framework_pages`, phase 5, step
  20): one `wiki/frameworks/{slug}.md` per canonical framework, complete
  `source_articles` + Publications list (provenance never LLM-truncated).
- LLM polish pass (`polish_framework_pages`): one orchestrator call per
  framework that sees EVERY naming article's usage note (each paper alone may
  carry only a partial explanation); runs outside the DB lock at the command
  layer, skeleton stays on failure (non-fatal).
- Connections: per-article synthesis pages + the wiki raw export render
  `[[framework-slug|name]]` / `## Theoretical Frameworks` from the same
  canonical field, so article-to-framework graph edges exist before the LLM
  batch ingest runs (the Existing Pages Index then pins the slugs).

### Ensure-summaries pre-phase (wikifix-final Change 1)

`wiki_rebuild` and `wiki_export_and_ingest` run `ensure_wiki_summaries`
(`commands/wiki_cmd/ingest.rs`) before the article load: included full-text
articles whose `full_text_ai_summary` is missing/empty get their blob
generated via `generate_article_ai_summary_inner`, so wiki export is
summary-scale for every article. Per-article failures are non-fatal
(`record_wiki_summary_failure` audit entry; abstract fallback). Skipped when
the LLM is not configured, and skipped unconditionally under `bango_ai` (v9;
`[wiki:diag] ensure-summaries skipped (Bango AI)`) because serialized local
full-text summaries are the longest pole and the abstract fallback is the
documented degraded input. Emits `wiki:progress` directly in the 1-9% slice
(outside the `prep_cb` 15-25% pre-seed range); cancel-checked between articles.

### Cancel-token + progress contract (v2, see `.worktrees/wiki2.md`)

All three entry points (`wiki_ingest`, `wiki_rebuild`,
`wiki_export_and_ingest`) snapshot a fresh `Arc<AtomicBool>` into the managed
`WikiIngestState` (`commands/wiki_cmd/mod.rs`, mirrors `ScrapingState`) at
start and clear it on return. The frontend `cancel_wiki_ingest` command signals
the active token. The pipeline checks `is_cancelled` between each of the 7
pre-seed steps in `build_batches_with_manifest` (on cancel: `Ok(Vec::new())` =
empty batches = no LLM calls) and on every progress tick plus between
`join_next().await` completions in
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
   main themes, publications, most cited, key references, research areas,
   collaborators). Enrichment (2026-09-16): Main Themes link `[[concept-slug]]`
   to the seeded concept hubs (tags outrank extracted terms); Most Cited ranks
   the author's own papers by `articles.num_cited` (top 5); Key References
   rank `reference_papers` by usage across the author's articles (type=1
   links, metadata-only); Research Areas are curated (blocklist + min length
   + cap 10). `collect_coauthors` was doubly broken behind a swallowed error
   (parameter count + name columns from the wrong table) and silently emptied
   the Collaborators section on every page; both are fixed and covered by
   `tests/wiki/wiki_author_enrichment_test.rs`.
2. `preseed_synthesis_from_ai_summaries` writes one
   `wiki/synthesis/{article_id}.md` per included article that has a
   `full_text_ai_summary` JSON blob - slug = article UUID (so `[[uuid]]` links
   resolve), body = author byline + `summary_150_250_words` digest +
   `key_insights` bullets, `tags` = keyword-derived `[[concept-slug]]`
   candidates. The byline links `[[author-slug|Name]]` per author, ordered by
   `biblio_article_authors.author_order` and slug-aligned with the pre-seeded
   author pages (plain-text fallback parsed from `articles.authors` when the
   biblio junction is empty); author slugs also join the `links` frontmatter.
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
   concepts. Tested in `tests/wiki/wiki_concepts_tags_test.rs` (11 tests).
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
`pages.rs`, `search_lint.rs`, `chat.rs`, `ingest.rs`, `site_export.rs`,
`obsidian_export.rs`; `pub
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
`blocking_pick_file` in the backend), and `wiki_export_obsidian`
(single-step Obsidian vault export: brief DB lock to resolve the wiki root +
load article rows ordered by `sequence_id`, then lock-free staging via
`wiki::obsidian_export::write_vault` + reuse of `site_export::zip_directory`
(temp zip + rename, cross-volume safe); emits `wiki:progress`; returns
`ObsidianExportResult { fileCount, path, stats }`).

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
`tests/wiki/wiki_test.rs::delete_wiki_de_initializes_by_removing_agents_md` +
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

- `tests/wiki/wiki_fts_test.rs` (36 unit tests)
- `tests/wiki/wiki_obsidian_export_test.rs` (Obsidian vault export: slug map,
  aliases, frontmatter/body rewrites, vault staging incl. the whole-vault
  no-UUID guard, orphan/collision fallbacks, zip round-trip; binding
  inventory rows in `docs/test-plans/wiki-export-tests.md`)
- `tests/wiki/wiki_ingest_test.rs` (65 tests: freeze, batching, local caps/ETA/tick cancel)
- `tests/wiki/wiki_consolidation_test.rs` (multi-batch consolidation)
- `tests/wiki/wiki_index_drift_test.rs` (two-tier external-edit drift detection)
- `tests/wiki/wiki_concepts_tags_test.rs` (11 tests)
- `tests/wiki/wiki_deterministic_test.rs` + `tests/wiki/wiki_methods_preseed_test.rs`
  (5-layer pre-seed)
- `tests/wiki/wiki_grounding_test.rs` (Tier A1 grounding gate)
- `tests/wiki/wiki_ensure_initialized_test.rs` (self-healing init guard)
- `tests/wiki/wiki_test.rs` (de-init on delete + staleness)
- `tests/wiki/wiki_full_text_refresh_test.rs` (staleness pairing)
- `src/__tests__/composables/use-wiki.test.ts` +
  `src/__tests__/components/wiki-toolbar.test.ts` +
  `src/__tests__/views/wiki-view.test.ts`
- inventory in `docs/test-plans/wiki-ingest-freeze-tests.md`

## Child DOX Index

- **`ingest/`** - LLM page generation (batching, consolidation, authors,
  synthesis, concepts, sources, methods, slugs). No own `AGENTS.md` yet; the
  contracts above cover the ingest pipeline.