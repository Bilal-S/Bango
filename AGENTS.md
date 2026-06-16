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

- **`src-tauri/src/`** — Rust backend (Tauri 2.x). Owned modules: `db/` (repos +
  `migrations/`), `models/`, `commands/`, `llm/` (orchestrator pattern), `screening/`,
  `dedup/`, `ris/`, `bibtex/`, `prisma/`, `export/`, `scraping/`, `crypto/`. App entry
  is `lib.rs` (`run()`), which registers all `#[tauri::command]` handlers in one
  `invoke_handler!` list and auto-loads the bundled `journal_index.db` on first startup.
  - **`src-tauri/src/db/biblio_repo/`** — bibliometric repos (`kpis`, `authors`,
    `networks`, `terms`, `institutions`, `normalization`, `productivity`). Contract:
    `get_biblio_kpis` returns `BiblioKpis` including `journal_distribution:
    Vec<JournalYearData>` (canonical titles via `journal_index` LEFT JOIN, fallback
    `UPPER(TRIM(journal))`). `productivity.rs` exposes `get_author_rankings`,
    `get_author_detail`, `get_author_productivity_kpis` — author-level h-index, i10,
    g-index, first/last/solo counts scoped to included articles. `networks.rs` exposes
    `get_cocitation_network_json` — on-demand co-citation computation with 4 normalization
    modes (Raw, Cosine, Jaccard, Pearson), `CocitationScope` (included/all articles).
  - **`src-tauri/src/db/journal_repo.rs`** — journal_index lookup/match (`resolve_journal_id`,
    `match_journal`, `get_journal_info`). `articles.journal_index_id` is populated on import
    and refreshable via the `rematch_journals` command.
  - **`src-tauri/tests/`** — Rust integration tests. Inline `#[cfg(test)] mod tests`
    blocks are extracted here to keep source files compact (helpers tested externally
    are `pub`). Repository/KPI tests live in `biblio_repo_tests.rs` (in-memory SQLite
    via `run_migrations`). Unit-test extractions: `biblio_normalizer_test.rs`,
    `biblio_models_test.rs`, `bibtex_parser_test.rs`, `bibtex_converter_test.rs`,
    `cr_parser_test.rs`, `doi_test.rs`, `n1_parser_test.rs`,
    `screening_engine_test.rs`, `pdf_extract_test.rs`, `browser_test.rs`.
- **`src/`** — Vue 3 + TypeScript + Tailwind v4 frontend.
  - **`src/views/`** — page-level views. `biblio-dashboard.vue` is the `/bibliometrics`
    parent; child routes (`coauthors`, `citations`, `keywords`, `timeline`, `authors`)
    render in its `<router-view>`. `biblio-timeline.vue` is the Publication Timeline view
    (its secondary "Top Journals" chart auto-hides below `SECONDARY_CHART_MIN_VIEWPORT_HEIGHT`
    = 700px viewport height, driven by the reactive `height` ref from `use-viewport.ts`);
    `biblio-authors.vue` is the Author Productivity Ranking view (sortable table + slide-over
    detail panel + Google Scholar external lookup icons).
  - **`src/components/`** — reusable components. `journal-info-card.vue` lazily loads
    journal metadata via the `biblio_get_journal_info` command.
  - **`src/composables/`** — Vue composables. `use-bibliometrics.ts` (shared KPI
    singleton, now exports `JournalYearData`), `use-journal-info.ts` (per-call lazy
    loader), `use-article-search.ts` (supports `yearFrom`/`yearTo`/`journal` route params).
  - **`src/utils/`** — pure utilities. `chart-export.ts` (timeline CSV/SVG export via the
    `save()` + `write_text_to_file` pattern shared with `network-export.ts`).
  - **`src/router/index.ts`** — route table; lazy views are prefetched after `router.isReady()`.
- **`docs/superpowers/specs/bango-v4-spec.md`** — authoritative v4 product specification.
- **`docs/design-reference/00-design-patterns.md`** — design tokens (Material 3 inspired).
- **`.worktrees/`** — planning documents (`biblio-publication-timeline-plan-v3.md` is the
  implemented plan; `biblio-cocitation-requirmenents.md` is the Co-Citation Analysis
  requirements spec; `biblio-plan.md` is the 8-screen bibliometric plan). Not part of the
  shipped app.

Verification gate: `npm run check:all` (type-check + eslint + prettier + rustfmt + clippy
`-D warnings`) and `cargo test`.
