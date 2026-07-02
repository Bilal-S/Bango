# Bango - Project Coding Rules

## General

- No unwrapped `unwrap()`, `expect()`, or panics in library/application code. Use `?` and proper error types.
- No `any` types in TypeScript. Use `unknown` and narrow with type guards if type is truly unknown.
- All code must pass `npm run check:all` before committing. Pre-commit hooks enforce this.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`.
  Never auto-add your agent name as a co-author (`Co-Authored-By:` trailer) to commit messages.
- Never manually modify `CHANGELOG.md` or any file marked as auto-generated.
- Never use the em dash character (`—`).
  Use a plain dash (`-`) instead in all generated text and comments.

## Engineering Principles

- When making technical decisions, do not over-weight development cost.
  Prefer quality, simplicity, robustness, scalability, and long-term maintainability.
- When writing or substantially editing long Markdown files, put each full sentence on its own physical line.
  Preserve normal Markdown structure, but avoid wrapping multiple sentences onto one physical line.

## Rust (src-tauri/)

### Error Handling
- Use `anyhow::Result` for application-level errors (Tauri commands, CLI).
- Use `thiserror` for library-level errors (RIS parsing, deduplication, LLM client).
- Never use `unwrap()` or `expect()` outside of tests. Clippy warns on both.
- Return `Result<T, E>` from all fallible functions.
- Use tauri-pilot mcp for E2E testing. Check whether dev server is running before attempting to start another instance.
- **System/Generic Error Logging**: For system-wide operational events or errors not tied to a specific article (e.g., scraping outcomes, global LLM client failures, database initialization errors), use `audit_repo::log_error(conn, details)`. This creates an audit entry with `article_id = NULL` and `action = 'error'`. Do not use this for article-specific events.

### Code Style
- Module structure: one module per domain concern (e.g., `ris`, `dedup`, `screening`, `llm`, `db`, `prisma`, `biblio`).
- Use `#[must_use]` on pure functions that return a value.
- No `clone()` unless the borrow checker truly requires it. Prefer references.
- Use `impl Trait` for return types in function signatures when appropriate.
- Prefer iterators over `for` loops with mutable accumulators.

### LLM Calls (Orchestrator Pattern)
- All LLM calls MUST go through `LlmOrchestrator` (registered as Tauri managed state).
- Never call `llm::client::send_chat_completion` directly from command handlers or engines.
- The orchestrator enforces `max_concurrent_requests` and `request_delay_ms` from `LlmConfig`.
- Use `LlmRequestType` enum to categorize requests for logging and diagnostics.
- Error logging to diagnostics and audit trail is handled centrally by the orchestrator.
- The `screening::llm_client::LlmClient` trait wraps the orchestrator for testability (mockable).

### Database (SQLite)
- Always use parameterized queries. Never interpolate user input into SQL.
- Use Tauri's SQL plugin or a dedicated module with prepared statements.
- Run migrations on app startup via a dedicated migration module.

### Naming
- Files and modules: `snake_case` (e.g., `ris_parser.rs`, `mod dedup_engine`)
- Types and structs: `PascalCase` (e.g., `Article`, `ScreeningResult`)
- Functions and variables: `snake_case` (e.g., `parse_ris`, `working_list`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_BATCH_SIZE`)

## TypeScript / Vue (src/)

### Component Style
- Always use `<script setup lang="ts">` with Composition API.
- No Options API. No `defineComponent()` - use `<script setup>` exclusively.
- One component per file. File name in `kebab-case`: `article-list.vue`, `screening-panel.vue`.

### Type Safety
- `strict: true` in tsconfig. No `any` - use `unknown` + type narrowing or proper interfaces.
- Define all API response shapes as TypeScript interfaces in a dedicated types file.
- Use `defineProps<T>()` with generic syntax for component props.

### Naming
- Components: `PascalCase` in templates and imports (`<ArticleList />`).
- Files: `kebab-case` (`article-list.vue`).
- Functions and variables: `camelCase` (`fetchArticles`, `workingList`).
- Constants: `SCREAMING_SNAKE_CASE` (`MAX_ARTICLES`, `DEFAULT_BATCH_SIZE`).
- Composables: prefixed with `use` (`useArticles.ts`, `useScreening.ts`).

### File Organization
- `src/components/` - reusable Vue components.
- `src/views/` - page-level components.
- `src/composables/` - Vue composables (shared reactive logic).
- `src/types/` - TypeScript interfaces and type definitions.
- `src/utils/` - pure utility functions (formatters, validators).
- `src/stores/` - Pinia stores for global state management.

## Security

- No secrets, API keys, or credentials in source code.
- API keys are encrypted with AES-256-GCM in local storage.
- Validate all user input at system boundaries (RIS import, criteria text, LLM config).
- Use `serde` deserialization with explicit field types - never deserialize untrusted input as `serde_json::Value` without validation.
- RIS parser must handle malformed input gracefully - never crash on bad data.

## Journal Index (Reference Data)

The `journal_index` table stores system-distributed reference data for journals.
It is populated from CSV files via the `scripts/import_journals/` Rust binary and
bundled as `src-tauri/resources/journal_index.db` with the Tauri app.

### Key Rules
- `journal_index` is **never** included in project backup export/import or reset.
- The table is marked `is_system = 1` for all bundled records.
- Blank CSV values never overwrite existing non-blank data on update.

### Startup Behavior (`lib.rs`)
1. Migrations run → `journal_index` table created (v001).
2. `load_journal_index_if_empty()` checks if the table has data.
3. If empty, ATTACHes the bundled `journal_index.db` and bulk-copies all records.

### How to Update Journal Data for a New Release
1. Place updated CSV files in `~/Documents/Journals/` (or a specified directory).
2. Run the import script:
   ```bash
   # Optionally delete the old DB first:
   rm src-tauri/resources/journal_index.db
   # Rebuild from all CSVs:
   cd scripts/import_journals && cargo run
   ```
3. Verify the new `src-tauri/resources/journal_index.db` is correct.
4. Create a new migration file to trigger a refresh on existing installations:
   - Create `src-tauri/src/db/migrations/v00N_refresh_journals.rs`:
     ```rust
     pub const VERSION: i32 = N;
     pub const UP_SQL: &str = "\
         -- Refresh journal index: clears table so bundled portal DB is reloaded.
         -- To update: run import_journals script with new CSVs, replace
         -- src-tauri/resources/journal_index.db, then bump this version.
         DELETE FROM journal_index;
     ";
     ```
   - Register it in `src-tauri/src/db/migrations/mod.rs`.
5. Commit the updated `journal_index.db` + the new migration file.

The migration `DELETE FROM journal_index` clears the table. On next app startup,
`load_journal_index_if_empty()` sees 0 records and reloads from the bundled DB.

### Supported CSV Formats (auto-detected by header)
- **Standard**: `"Journal title","ISSN","eISSN","Publisher name","Publisher address","Languages","Web of Science Categories"`
- **JCR**: `Title20,Title,Country,SCIE,SSCI,AHCI,ESCI`

### Match Priority (import script)
1. ISSN (exact) → 2. eISSN (exact) → 3. Title (case-insensitive) → 4. New record

## Testing

- Rust: `cargo test` for unit and integration tests.
- TypeScript: Vitest for unit tests.
- Test file naming: `*.test.ts` for TS, `*_test.rs` or inline `#[cfg(test)]` for Rust.
- Place Rust integration and database repository tests in the `src-tauri/tests/` directory.
- Avoid large inline unit tests in library source files (e.g. database repository modules); instead, move them into standalone integration test files under `src-tauri/tests/` to keep the source code files compact and maintainable.

### Coverage Goals

- **Target: 70% line coverage for both Rust (`src-tauri/`) and Vue/TS (`src/`).**
- Coverage is **opt-in and not part of `npm run check:all`** (which runs type-check +
  eslint + prettier + rustfmt + clippy + the plain Vitest suite). Run it on demand:
  - Vue/TS: `npm run test:coverage` runs `vitest run --coverage`, which enforces the
    `vitest.config.ts` `coverage.thresholds` block and writes `coverage/index.html`
    (via `@vitest/coverage-v8`).
  - Rust: `npm run coverage:rust` runs `cargo llvm-cov --fail-under-lines 70`
    (requires `cargo-llvm-cov` + the `llvm-tools-preview` rustup component).
- Tooling & reproduction:
  - Vue/TS: `npm run test:coverage` -> report at `coverage/index.html`.
  - Rust: `cd src-tauri && cargo llvm-cov --html --output-dir target/llvm-cov/html`
    -> report at `target/llvm-cov/html/html/index.html`.
  - Both artifact dirs are git-ignored.
- See `docs/test-coverage-report.md` for the current baseline and the ranked list of
  highest-value coverage gaps.

### Coverage Strategy

- **Prefer testing extracted logic over `#[tauri::command]` shims.** Command handlers
  require Tauri `State<DbState>` and cannot be unit-tested directly. Extract
  non-trivial orchestration into `pub fn`s that accept `&Connection` (or pure inputs)
  and test those; keep the command wrapper thin.
- **Vue component tests**: mount via `@vue/test-utils` with a shared helper that mocks
  `tauriCommand`, Pinia, and the router. Stub canvas/chart libraries (sigma, apexcharts)
  to focus assertions on logic and template branches.
- **Cover the cheap, pure layers first**: `src/utils/*`, `src/stores/*`, and pure Rust
  models/repos yield high line coverage per unit of effort; defer heavy view/graph
  components until last.
- When adding or changing source code, add or update tests in the same change so
  coverage does not regress.
- **Test-First Protocol for multi-tier plans.** When a feature is specified in a
  planning doc (`.worktrees/*.md`) with a Test Inventory section (tables whose
  rows are `file::function` identifiers), the inventory is binding: every listed
  `file::function` test must exist (un-ignored, passing) before the tier's PR
  merges. Tiers ship in two PRs - a prep PR that adds the inventory as
  `#[ignore]` (Rust) / `it.skip` (TS) stubs, and an implementation PR that
  un-ignores each test as it lands. Reviewers grep for tier-labeled leftovers
  (`grep -rn "TODO: tier" src-tauri/tests/ src/__tests__/`) - any leftover blocks
  the PR. `scripts/check-test-inventory.sh` enforces this mechanically by parsing
  the plan doc's inventory tables and grepping the named test files; it is wired
  into `npm run check:all` via the `check:test-inventory` script.
- **Tier 3 screening modes test coverage.** The three `screening_mode` values
  (`abstract`, `enhanced`, `two_stage`) must each have at least one integration
  test exercising the end-to-end engine path (`run_sync` with a `ScreeningConfig`
  carrying that mode + a mock `LlmClient`). The pure retrieval layer
  (`screening::chunk_retrieval::rank_chunks_by_criteria`) MUST be unit-tested
  independently of the engine (it has no I/O). The pure prompt helper
  (`screening::prompt::build_screening_prompt`) must have tests asserting the
  `## Supporting Evidence from Full Text` block is present when
  `ArticleEntry.full_text_evidence = Some(...)` and absent when `None` (backward
  compat for abstract-mode prompts). Two-stage tests must verify that clear-cut
  confidence values skip stage 2 and borderline values trigger it, plus that
  borderline articles carry both `ai_screen` and `ai_screen_enhanced` audit
  entries.

### Bug Fixes & Engineering Hygiene

- When doing bug fixes, always start with reproducing the bug in an E2E setting
  as closely aligned with how an end user would experience it as possible.
  This makes sure you find the real problem so your fix will actually solve it.
- Apply the same high standard to engineering excellence: lint, test failures, and test flakiness.
  If you encounter one - even if it is not caused by what you are working on right now - still get it fixed.

## Tauri App Diagnostics & Testing

- When diagnosing frontend behavior, viewports, UI latency, or screen freezes, the agent can run the Tauri desktop app and use the `tauri-pilot` MCP tools to:
  - Interactively navigate the application (`mcp_tauri-pilot_navigate`).
  - Perform clicks and element queries (`mcp_tauri-pilot_click`, `mcp_tauri-pilot_wait`, `mcp_tauri-pilot_text`).
  - Retrieve current state, console logs, and network logs (`mcp_tauri-pilot_logs`, `mcp_tauri-pilot_network`).
  - Capture screenshots to visually inspect layout issues (`mcp_tauri-pilot_screenshot`).
- When end-to-end testing a product, be picky about the UI you see and be obsessed with pixel perfection.
  If something clearly looks off, even if it is not directly related to what you are doing, inform user to approve the fix along with the current task.
