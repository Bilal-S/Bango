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

- **Frontend "is the LLM configured?" gate - single canonical pattern**:
  every frontend feature gate that depends on "LLM configured" MUST read
  `useLlmConfigured()` (which wraps the `llm-config` Pinia store); no
  component, view, composable, or store may derive its own LLM-configured
  state. Full contract: `src/AGENTS.md` §Local Contracts.

## Child DOX Index

Top-level source directories. Each entry below is a **pointer** to the nearest
owning AGENTS.md; follow it for the detailed contracts. Create a child
`AGENTS.md` under a folder only when that folder grows its own local rules.

- **`src-tauri/src/`** - Rust backend (Tauri 2.x). Owns the article state
  machine, hard-delete cascade, journal-index loader, startup upgrade path,
  and the backend Child DOX Index. See `src-tauri/src/AGENTS.md`. Child docs:
  `commands/`, `db/`, `llm/`, `screening/`, `wiki/`, `embedding/`,
  `citation_finder/`, `translation/`, `batch_import/`, `openalex/`,
  `scraping/`, `export/`, `utils/`.
- **`src-tauri/tests/`** - Rust integration tests (one self-contained crate
  per `.rs` file; also the extraction home for inline `#[cfg(test)]` blocks).
  See `src-tauri/tests/AGENTS.md`.
- **`src/`** - Vue 3 + TypeScript + Tailwind v4 frontend. Owns the canonical
  LLM-configured gate, the multi-source `watch()` rule, and the shared test
  helpers, plus the frontend Child DOX Index: `views/`, `components/`,
  `composables/`, `stores/` each have their own `AGENTS.md`; `utils/`,
  `types/`, `router/`, `styles/`, `workers/` are indexed inline. See
  `src/AGENTS.md`.
- **`landingpage/`** - standalone marketing microsite (NOT part of the shipped
  Tauri app). Static HTML5 + Tailwind v4 (browser CDN build). Two pages
  (`index.html` + `help.html`); shared `assets/`. When porting app Help
  content to `help.html`, remove app-only interactivity (Vue router
  navigation, demo-project loader, scroll-spy) and replace CSS variables /
  Tailwind-scoped styles with plain CSS or self-contained utility classes.
- **`tests/test-citations/`** - RIS fixture data for citation/reference system
  tests. `main_articles.ris` (10 articles, DOIs `10.1001/art1`–`10.1010/art10`)
  with per-article `_references.ris` and `_citations.ris` files (filename =
  DOI with `/`→`_`). A dedicated co-citation dataset uses `co-citation.ris`.
- **`docs/bango-v4-spec.md`** - authoritative v4 product specification.
- **`docs/CLAUDE.md`** - project coding rules (Rust/TS error handling, naming,
  LLM orchestrator pattern, DB rules, testing conventions).
- **`docs/test-coverage-report.md`** - coverage baseline + under-coverage
  analysis for Rust and Vue/TS.
- **`docs/design-reference/00-design-patterns.md`** - design tokens (Material 3
  inspired).
- **`docs/test-plans/`** - binding test inventory files consumed by
  `scripts/check-test-inventory.sh` (wired into `npm run check:all`).
- **`.worktrees/`** - planning documents. Not part of the shipped app.

Verification gate: `npm run check:all` (type-check + eslint + prettier + rustfmt + clippy
`-D warnings` on the library crate + vitest + `check:test-inventory`) and
`cargo test`. Clippy test-awareness details live in `docs/CLAUDE.md`
§Error Handling; per-side coverage tooling and the `src-tauri/target`
disk-space contract live in the `src/AGENTS.md` and `src-tauri/src/AGENTS.md`
Verification sections.
