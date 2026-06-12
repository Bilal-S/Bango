# Bango - Project Coding Rules

## General

- No unwrapped `unwrap()`, `expect()`, or panics in library/application code. Use `?` and proper error types.
- No `any` types in TypeScript. Use `unknown` and narrow with type guards if type is truly unknown.
- All code must pass `npm run check:all` before committing. Pre-commit hooks enforce this.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`

## Rust (src-tauri/)

### Error Handling
- Use `anyhow::Result` for application-level errors (Tauri commands, CLI).
- Use `thiserror` for library-level errors (RIS parsing, deduplication, LLM client).
- Never use `unwrap()` or `expect()` outside of tests. Clippy warns on both.
- Return `Result<T, E>` from all fallible functions.
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
- Place Rust tests in a `tests/` directory beside the module they test.

## Tauri App Diagnostics & Testing

- When diagnosing frontend behavior, viewports, UI latency, or screen freezes, the agent can run the Tauri desktop app and use the `tauri-pilot` MCP tools to:
  - Interactively navigate the application (`mcp_tauri-pilot_navigate`).
  - Perform clicks and element queries (`mcp_tauri-pilot_click`, `mcp_tauri-pilot_wait`, `mcp_tauri-pilot_text`).
  - Retrieve current state, console logs, and network logs (`mcp_tauri-pilot_logs`, `mcp_tauri-pilot_network`).
  - Capture screenshots to visually inspect layout issues (`mcp_tauri-pilot_screenshot`).
