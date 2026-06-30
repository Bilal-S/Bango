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

Top-level source directories. No child `AGENTS.md` files exist yet; these entries
describe each durable boundary so agents can locate the right area. Create a child
`AGENTS.md` under a folder only when that folder grows its own local rules.

- **`src-tauri/src/`** - Rust backend (Tauri 2.x). Owned modules: `db/` (repos +
  `migrations/`), `models/`, `commands/`, `llm/` (orchestrator pattern), `screening/`,
  `dedup/`, `ris/`, `bibtex/`, `prisma/`, `export/`, `scraping/`, `crypto/`, `wiki/`
  (LLM knowledge base; see `wiki/` entry below), `utils/` (pure helpers:
  `pdf_extract.rs`, `sections.rs`, `chunking.rs`). App entry
  is `lib.rs` (`run()`), which registers all `#[tauri::command]` handlers in one
  `invoke_handler!` list and auto-loads the bundled `journal_index.db` on first startup.
  - **`src-tauri/src/db/app_settings_repo.rs`** - key/value `app_settings` store. Holds
    `fulltext_storage_dir`, `flag_premium`, `biblio_needs_refresh` (the bibliometric
    staleness flag), and `wiki_needs_refresh` (the LLM Wiki staleness flag). `mark_biblio_needs_refresh(conn)` is called by every mutation that
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
    `fts.rs` (FTS5 BM25 search index + **two-tier external-edit drift detection**), `ingest.rs` (LLM page generation: prompt builder,
    `<!-- PAGE:slug -->` response parser, page writer, FTS5 rebuild, **parallel chunked
    ingest**), `engine.rs` (deterministic lint + `build_graph` for link graph
    visualization), `chat.rs` (token-budgeted RAG chat over FTS5 index; self-heals the
    FTS table via `fts::ensure_index_populated` when the index is empty OR its row count
    mismatches the number of `.md` pages on disk).
    **Parallel chunked ingest** (`ingest.rs`): `wiki_ingest`, `wiki_rebuild`, and
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
    `build_ingest_prompt_batches`, `run_chunked_ingest`. The legacy single-call
    `build_ingest_prompt` + `write_pages_from_response` remain for backward compat and
    regression tests.
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
    **Deterministic 4-layer pre-seed matrix** (`build_batches_with_manifest` in
    `commands/wiki_cmd.rs`, runs unconditionally before the LLM on every
    single-batch AND multi-batch run): (1) `preseed_authors` writes rich author
    pages from `biblio_authors` (metrics, publications, research areas,
    collaborators); (2) `preseed_synthesis_from_ai_summaries` writes one
    `wiki/synthesis/{article_id}.md` per included article that has a
    `full_text_ai_summary` JSON blob — slug = article UUID (so `[[uuid]]` links
    resolve), body = `summary_150_250_words` digest + `key_insights` bullets,
    `tags` = keyword-derived `[[concept-slug]]` candidates; (3)
    `preseed_concept_hubs` writes top-25 `wiki/concepts/{term-slug}.md` hub
    pages from `biblio_terms`, each linking to its articles (`[[uuid]]`) +
    co-occurring concepts; (4) **`preseed_document_source_pages`** writes one
    `wiki/sources/{user-slug}.md` per user-uploaded document (Add Documents →
    PDF/TXT/web, identified by `source_kind: user_*`) so external documents get
    a first-class wiki node and `[^art-user-slug]` / `[[user-slug]]` citations
    resolve to a navigable page instead of "Page not found". This layer mirrors
    the article→synthesis symmetry: every raw source has a corresponding wiki
    node. All four respect `status: reviewed` (user-edited) pages. Together they
    form a connected graph backbone (author ↔ synthesis ↔ concept ↔ source) that
    exists before the LLM runs, so the wiki is never missing
    author/synthesis/concept/source pages regardless of which LLM model is used.
    Tested in `wiki_deterministic_test.rs`. Design + phases 4-5 (LLM prompt
    narrowing, `concepts` field in AI summary schema) in
    `.worktrees/wiki-improvement-plan.md`; external-document ingestion +
    linking design in `.worktrees/wiki-improvement-plan2.md`.
    `commands/wiki_cmd.rs` exposes
    all Tauri commands: `wiki_get_status`, `wiki_init`, `wiki_export_raw`,
    `wiki_add_raw_file`, `wiki_list_raw_files`, `wiki_search`, `wiki_lint`,
    `wiki_get_page`, `wiki_update_page`, `wiki_delete_page`, `wiki_delete_wiki`,
    `wiki_chat`, `wiki_get_graph`, `wiki_ingest`, `wiki_list_pages`, `wiki_list_sources`,
    `wiki_rebuild` (one-click full pipeline: scaffold + export + ingest + FTS5, emits
    `wiki:progress` events), `wiki_export_and_ingest` (export + ingest after Add Documents),
    and `wiki_check_for_updates`. `wiki_search` rebuilds the FTS index if empty;
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
    Design and phasing: `.worktrees/llmwiki-plan.md`.
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
  - **`src-tauri/src/db/journal_repo.rs`** - journal_index lookup/match (`resolve_journal_id`,
    `match_journal`, `get_journal_info`). `articles.journal_index_id` is populated on import
    and refreshable via the `rematch_journals` command.
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
  - **`src-tauri/src/export/project.rs`** - `ProjectBackup` serialize/deserialize. Exports only
    source tables (aims, criteria, articles, tags, labels, article_tags/labels, audit,
    reference_papers, article_reference_links, llm_config). The 9 `biblio_*` tables are NOT
    exported - they are dynamically generated by `biblio_normalize` and would bloat backups
    and trigger UNIQUE constraint violations on import. After import,
    `mark_biblio_needs_refresh` ensures the frontend auto-regenerates them. The import code
    uses `INSERT OR IGNORE` + ID-remap maps for `reference_papers`, `biblio_authors`,
    `biblio_institutions`, and `biblio_terms` (all have UNIQUE constraints) to handle older
    backups that may still contain biblio data.
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
    absent-key default). `wiki_full_text_refresh_test.rs` covers the wiki staleness-flag
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
    `chunking_test.rs` is inline in `utils/chunking.rs` (9 tests: empty input,
    short section, references skip, long-section sentence split, tiny-tail
    merge, Text label, Heading label, contiguous chunk_index, empty-body skip).
  - **`src-tauri/src/utils/sections.rs`** - section-aware text classification (T1.1).
    `classify_sections(text)` splits flat extracted text into `Vec<Section>` by detecting
    heading lines (markdown `##`, numbered `2.1 Study Design`, bare keyword `METHODS`).
    `SectionKind` enum: `Heading, Abstract, Introduction, Methods, Results, Discussion,
    Conclusion, References, Text` (Table/Figure deferred to T2). `SectionKind::label()`
    returns the stable display string for each variant (used by T1.3 prompt builders +
    UI rendering). `extract_sections(path)` is the I/O wrapper that runs
    `extract_pdf_text`/`extract_txt_text` then classifies. Pure functions (`#[must_use]`);
    the proven `strip_abstract`/`strip_references` in `pdf_extract.rs` are kept unchanged
    (new consumers call `classify_sections` directly). Consumed by T1.2 `chunking.rs`
    and T1.3 `commands::summary::generate_article_ai_summary` (section-aware branch via
    `summary::prompt::{filter_high_value_sections, build_section_context}`).
  - **`src-tauri/src/utils/chunking.rs`** - semantic chunking (T1.2). `chunk_sections(
    sections, target_words)` walks `Section`s and emits `Vec<Chunk>` bounded by
    `DEFAULT_CHUNK_WORDS=512` / `MIN_CHUNK_WORDS=100` / `MAX_CHUNK_WORDS=1200`. Splits
    long sections at sentence boundaries; merges tiny tails; skips `References` entirely;
    carries section provenance (`Some("Methods")`) so FTS5 chunk rows + chat citations
    can render `(§Methods)`. Pure functions (`#[must_use]`). Consumed by `wiki::fts`
    (planned: `collect_page_rows` chunk-emission) and T3.1 `attach_full_text` chunk storage.
  - **`src-tauri/src/db/migrations/v003_fts_sections.rs`** - Tier 1-4 schema (VERSION 3).
    Two changes: (1) `DROP TABLE IF EXISTS wiki_pages_fts;` so `fts::ensure_table`
    recreates it with chunk-aware columns (`chunk_index`, `section`, `parent_slug` UNINDEXED)
    on the next read (FTS5 virtual tables cannot be `ALTER`ed; the explicit DROP is the
    supported schema-change path, and the table self-heals via `ensure_index_populated`);
    (2) `CREATE TABLE article_chunks` (per-article chunk storage populated at attach time
    by T3.1, consumed by screening T3.2+). No `ALTER TABLE articles` - section summaries
    (T1.3) live inside the existing `full_text_ai_summary` column as a `schema_version: 2`
    superset blob.
  - **`src-tauri/src/wiki/fts.rs`** (T1.2 update) - chunk-aware FTS5 schema:
    `ensure_table` now creates `chunk_index UNINDEXED, section UNINDEXED, parent_slug
    UNINDEXED` columns. `PageRow` carries `chunk_index: Option<i32>`, `section:
    Option<String>`, `parent_slug: Option<String>`. `WikiPageHit` surfaces the same three
    fields. `ensure_index_populated` self-heal compares `COUNT(DISTINCT COALESCE(
    parent_slug, slug))` against disk page count (not raw row count) so chunk rows do
    not false-positive a rebuild on every chat call. `search` SELECT + row mapping
    updated for the 3 new columns.
  - **`src-tauri/src/wiki/chat.rs`** (T1.2 update) - chunk-aware context builder:
    `MAX_HITS` raised from 8 to 16. `build_context` dedupes by `parent_slug` (keeps
    top-ranked chunk per page, appends "(+N more passages from this page)"). `format_entry`
    includes `(§Methods)` in the header when `hit.section` is present so the model can cite
    the passage. 3 new tests: section-label-in-header, dedupe-chunks-of-same-page,
    distinct-pages-not-deduped.
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
  - **`src/views/`** - page-level views. `biblio-dashboard.vue` is the `/bibliometrics`
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
  - **`src/components/`** - reusable components. `journal-info-card.vue` lazily loads
    journal metadata via the `biblio_get_journal_info` command. `help/` holds the five
    `help-tab-*.vue` tab components consumed by `help-guide.vue`; shared card styles live in
    `src/styles/help-shared.css`. `settings/` holds the five settings sub-components consumed by
    `settings-view.vue`: `settings-provider-card.vue` (consolidated AI Provider box - warning +
    connection details + parameters + Revert/Get Models/Test Connection + test-result/error
    feedback in one bordered `<section>`), `settings-project-management.vue` (import/export/delete
    + dialogs; Delete All Data also wipes the on-disk Wiki and resets
    `useWiki`/`useChatStore.wikiReady`; Export dialog warns that the Bango Documents
    directory - full-text PDFs + Wiki - is NOT backed up), `settings-screening-preferences.vue` (3 localStorage-backed toggles:
    auto-navigate-after-decision, full-text-summaries [auto-fire whole-paper summary on attach],
    section-summaries [T1.3: auto-fire per-section summaries on attach; independent of
    full-text-summaries; manual `auto_awesome` button always works regardless]),
    `settings-full-text-storage.vue` (storage dir picker), `settings-diagnostics.vue` (error log).
    Shared card chrome for these lives in `settings-card-shared.css`.
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
  - **`src/utils/`** - pure utilities: `network-export.ts` (graph PNG/GEXF export via the
    `save()` + `write_text_to_file` pattern), `formatters.ts`, `color.ts`, `debounce.ts`,
    `next-paint.ts`, `reference-flatten.ts`, `citation-analysis.ts`, `llm-error.ts`,
    `google-trends.ts` (Trends embed URL builder + date-range validators),
    `wiki-markdown.ts` (shared wiki Markdown renderer: `renderWikiMarkdown(text, opts?)`
    converts `[[slug]]` / `[[slug|alias]]` to `.wikilink` anchors and `[^art-id]`
    footnotes to `.art-ref` anchors (with `data-slug` / `data-art-id` attrs).
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
- **`.worktrees/`** - planning documents (`biblio-publication-timeline-plan-v3.md` is the
  implemented plan; `biblio-cocitation-requirmenents.md` is the Co-Citation Analysis
  requirements spec; `biblio-plan.md` is the 8-screen bibliometric plan). Not part of the
  shipped app.

Verification gate: `npm run check:all` (type-check + eslint + prettier + rustfmt + clippy
`-D warnings`) and `cargo test`.

Coverage tooling: `npm run test:coverage` (Vue/TS via `@vitest/coverage-v8`, config in
`vitest.config.ts`, report at `coverage/index.html`) and
`cd src-tauri && cargo llvm-cov --html --output-dir target/llvm-cov/html` (Rust via
`cargo-llvm-cov` + `llvm-tools-preview`, report at
`src-tauri/target/llvm-cov/html/html/index.html`). Both artifact dirs are git-ignored.
