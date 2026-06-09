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

## Testing

- Rust: `cargo test` for unit and integration tests.
- TypeScript: Vitest for unit tests.
- Test file naming: `*.test.ts` for TS, `*_test.rs` or inline `#[cfg(test)]` for Rust.
- Place Rust tests in a `tests/` directory beside the module they test.
