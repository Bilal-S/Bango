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
  (LLM knowledge base; see `wiki/` entry below). App entry
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
  - **`src-tauri/src/wiki/`** - LLM Wiki knowledge-base module (all phases complete).
    Generates and maintains a local-first Obsidian-style Markdown knowledge base from the
    `included` article corpus. Modules: `storage.rs` (resolves `wiki-root/`, scaffolds
    `raw/`, `wiki/{concepts,authors,methods,synthesis}/`, `templates/`, `AGENTS.md`,
    `log.md`), `agents_contract.rs` (ingest + lint rules contract), `templates.rs` (page
    templates), `frontmatter.rs` (dependency-free YAML parser/serializer),
    `raw_export.rs` (included-article export + user-file extraction for PDF/TXT/HTML/etc),
    `fts.rs` (FTS5 BM25 search index), `ingest.rs` (LLM page generation: prompt builder,
    `<!-- PAGE:slug -->` response parser, page writer, FTS5 rebuild),
    `engine.rs` (deterministic lint + `build_graph` for link graph visualization),
    `chat.rs` (token-budgeted RAG chat over FTS5 index). `commands/wiki_cmd.rs` exposes
    all Tauri commands: `wiki_get_status`, `wiki_init`, `wiki_export_raw`,
    `wiki_add_raw_file`, `wiki_list_raw_files`, `wiki_search`, `wiki_lint`,
    `wiki_get_page`, `wiki_update_page`, `wiki_delete_page`, `wiki_delete_wiki`,
    `wiki_chat`, `wiki_get_graph`, `wiki_ingest`, `wiki_list_pages`, `wiki_list_sources`,
    `wiki_rebuild` (one-click full pipeline: scaffold + export + ingest + FTS5, emits
    `wiki:progress` events), `wiki_export_and_ingest` (export + ingest after Add Documents).
    The `wiki_needs_refresh` flag triple lives in `app_settings_repo.rs`; cleared after
    `wiki_ingest`/`wiki_rebuild` commits. Frontend: `wiki-view.vue` (sidebar + viewer +
    editor + graph + article detail slide-over), `wiki-toolbar.vue` (Re-scaffold, Add
    Documents, Lint, Delete Wiki, progress bar), `wiki-page-viewer.vue` (Markdown +
    `[[wikilink]]` + `[^art-id]` source ref resolution), `wiki-page-editor.vue` (split-pane
    editor), `wiki-graph-panel.vue` (sigma + ForceAtlas2 graph). Composable: `use-wiki.ts`.
    Design and phasing: `.worktrees/llmwiki-plan.md`.
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
    `commands::export_cmd::reset_project` and the legacy upgrade path.
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
    absent-key default). `legacy_upgrade_test.rs` covers the full legacy upgrade round-trip
    (legacy article_references -> backup -> rebuild -> import) plus the
    `legacy_upgrade_needed(live, fallback)` pure decision function (live-probe-wins and
    snapshot-fallback branches).
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
    documents all six completed modules. `wiki-view.vue` is the `/wiki` route (flat, below
    `/chat` in `nav-sidebar.vue` with the `local_library` icon). Ships the empty-state gates
    (LLM configured, included articles > 0, wiki initialized), the sidebar (page list
    grouped by type + client-side search filter), the page viewer (`wiki-page-viewer.vue`
    with `[[wikilink]]` + `[^art-id]` source ref resolution + article detail slide-over),
    the split-pane editor (`wiki-page-editor.vue`), the sigma graph view
    (`wiki-graph-panel.vue` with ForceAtlas2 layout, color-coded by page type), and the
    toolbar (`wiki-toolbar.vue`: Re-scaffold one-click pipeline, Add Documents, Lint, Delete
    Wiki, progress bar). Composable: `use-wiki.ts`; types: `types/wiki.ts`.
  - **`src/components/`** - reusable components. `journal-info-card.vue` lazily loads
    journal metadata via the `biblio_get_journal_info` command. `help/` holds the five
    `help-tab-*.vue` tab components consumed by `help-guide.vue`; shared card styles live in
    `src/styles/help-shared.css`. `settings/` holds the five settings sub-components consumed by
    `settings-view.vue`: `settings-provider-card.vue` (consolidated AI Provider box - warning +
    connection details + parameters + Revert/Get Models/Test Connection + test-result/error
    feedback in one bordered `<section>`), `settings-project-management.vue` (import/export/delete
    + dialogs), `settings-screening-preferences.vue` (2 localStorage-backed toggles),
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
  - **`src/utils/`** - pure utilities: `network-export.ts` (graph PNG/GEXF export via the
    `save()` + `write_text_to_file` pattern), `formatters.ts`, `color.ts`, `debounce.ts`,
    `next-paint.ts`, `reference-flatten.ts`, `citation-analysis.ts`, `llm-error.ts`,
    `google-trends.ts` (Trends embed URL builder + date-range validators).
  - **`src/styles/forms.css`** - global form/button/dialog primitives (`.field__*`, `.btn--*`,
    `.dialog`, `.spinner`) promoted from the former scoped `llm-config.vue`. Loaded via
    `base.css`; low specificity so scoped rules in other views still win.
  - **`src/router/index.ts`** - route table; lazy views are prefetched after `router.isReady()`.
    `/settings` renders `settings-view.vue`.
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
