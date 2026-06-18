# Test Coverage Report

Coverage baselines captured via `cargo-llvm-cov` (Rust) and `@vitest/coverage-v8` (Vue/TS). Generated 2026-06-17; updated with Phase 1-6 coverage work.

## How to reproduce

```bash
# Rust (HTML report + per-file summary)
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
cd src-tauri && cargo llvm-cov --html --output-dir target/llvm-cov/html
# Open: src-tauri/target/llvm-cov/html/html/index.html

# Vue/TS
npm run test:coverage
# Open: coverage/index.html
```

Coverage artifacts are git-ignored (`coverage/`, `src-tauri/target/`). The `@vitest/coverage-v8` devDependency and `test:coverage` script are in `package.json`; the `coverage` block is in `vitest.config.ts`.

**Coverage goal: 70% lines for both stacks** (see `docs/CLAUDE.md` > Testing > Coverage Goals). Opt-in (NOT part of `npm run check:all`, which runs the plain Vitest suite): run `npm run test:coverage` (Vue, enforces `vitest.config.ts` thresholds) or `npm run coverage:rust` (`cargo llvm-cov --fail-under-lines 70`).

---

## Headline numbers

| Stack | Baseline (initial) | Current | Target |
|-------|--------------------|---------|--------|
| **Rust** (`src-tauri/`) | 51.93% lines | **~56.5%** lines | 70% |
| **Vue/TS** (`src/`) | 17.57% lines | **~26.5%** lines | 70% |

Rust coverage is dominated by well-tested pure-logic modules (parsing, dedup, biblio networks). The low function % comes from untested `#[tauri::command]` shims.

---

## Progress log (coverage improvement work)

### Rust (51.93% → ~56.5%)

New test files added:
- `src-tauri/tests/models_test.rs` - model `as_str()`/`Display`/`Default` impls (article, criterion, label, tag, llm_config).
- `src-tauri/tests/token_estimation_test.rs` - `estimate_tokens`, `check_context_window`.
- `src-tauri/tests/error_test.rs` - `AppError` variants + `Serialize` impl.
- `src-tauri/tests/llm_config_repo_test.rs` - `get_config`, `save_config`, `has_config`, `get_config_no_decrypt` (all provider variants, key round-trip).
- `src-tauri/tests/summary_repo_test.rs` - `save_summary` upsert, `get_summary`, `clear_summary`.
- `src-tauri/tests/journal_repo_test.rs` - `match_journal` (ISSN/eISSN/name/empty), `resolve_journal_id`, `get_journal_info` (aggregates + exclusion).
- `src-tauri/tests/prisma_svg_test.rs` - `render_prisma_svg` (structure, ongoing phase, exclusion reasons, truncation, XML escaping).
- `src-tauri/tests/import_pipeline_test.rs` - `read_content`, `parse_and_validate` (strict/none), preview building, `filter_excluded`.
- `src-tauri/tests/summary_engine_test.rs` - `generate_summary` (empty, single, batched, trim, error propagation).

### Vue/TS (17.57% → ~26.5%)

New test infrastructure:
- `src/__tests__/helpers/mount.ts` - shared `mountComponent()` with Pinia + router.
- `src/__tests__/helpers/fixtures.ts` - `makeArticle()` factory + `shimLocalStorage()` for happy-dom.

New test files:
- Utils: `debounce.test.ts`, `formatters-extended.test.ts`, `color-extended.test.ts`.
- Stores: `audit-store`, `articles-store`, `criteria-store`, `trends-queue-store`, `llm-config-store`.
- Router: `router.test.ts`.
- Composables: `use-feature-flags`, `use-viewport`, `use-tauri-command`, `use-toast`, `use-demo`, `use-llm-config`.
- Components: `confidence-bar`, `screening-stats`, `detail-header`, `screening-progress-bar`, `audit-timeline`, `matched-criteria`, `article-metadata`, `ai-decision-card`, `article-notes`.

---

## Remaining gaps (to reach 70%)

### Rust (~14% gap)
The largest remaining block is the **`commands/*.rs` shims** (~2633 lines at 0%). These require Tauri `State<DbState>` and cannot be unit-tested directly. Strategy (documented in `docs/CLAUDE.md`): extract non-trivial orchestration into `pub fn`s accepting `&Connection`, test those, keep command wrappers thin. Additional DB-repo deepening (`article_repo`, `reference_repo`, `export/project`) and `utils/pdf_extract` expansion will close the rest.

### Vue/TS (~44% gap)
- **All 19 views remain untested** (template-heavy; need shallow mount with stubbed children). Infrastructure is ready via `mountComponent()`.
- **~35 components remain at 0%** - presentational components are high-ROI for mounting tests.
- **Network/graph composables** (`use-sigma-renderer`, `use-network-layout`, `use-citation-network`, etc.) need sigma/graphology mocking.
- **Branch coverage** is weakest; many tests cover happy paths only.

---

## Original baseline analysis (for reference)

<details>
<summary>Initial 0%-coverage lists (before improvement work)</summary>

### Rust - Tier 1 (0% line coverage)

**Command shims:** `commands/app_settings.rs`, `articles.rs`, `biblio_cmd.rs`, `chat.rs`, `criteria.rs`, `dedup.rs`, `export_cmd.rs`, `full_text.rs`, `labels.rs`, `llm_config.rs`, `mod.rs`, `prisma.rs`, `scraping.rs`, `screening.rs`, `startup.rs`, `summary.rs`, `tags.rs`, `trends.rs`.

**Business logic (now covered or improved):** `summary/engine.rs` (was 0%, now ~97%), `prisma/svg.rs` (was 0%, now covered), `ris/import_pipeline.rs` (was 0%, now covered), `db/summary_repo.rs` (was 0%, now covered), `error.rs` (was 0%, now covered).

**Models (now covered):** `article.rs`, `criterion.rs`, `label.rs`, `tag.rs`, `llm_config.rs` (all were 0%, now covered via `models_test.rs`).

### Vue/TS - Tier 1 (0% coverage)

**Utils (now covered):** `debounce.ts` (was 0%), `formatters.ts` (was 36%), `color.ts` (was 61%).

**Stores (now covered):** `trends-queue.ts` (was 0%), `audit.ts` (was 4%), `criteria.ts` (was 62%), `llm-config.ts` (was 60%), `articles.ts` (was 75%).

**Router (now covered):** `router/index.ts` (was 0%).

**Composables (now covered):** `use-feature-flags`, `use-viewport`, `use-tauri-command`, `use-toast`, `use-demo`, `use-llm-config` (all were 0%).

**Components (now covered):** `confidence-bar`, `screening-stats`, `detail-header`, `screening-progress-bar`, `audit-timeline`, `matched-criteria`, `article-metadata`, `ai-decision-card`, `article-notes` (all were 0%).

</details>

## Notes / caveats

- **Command shims inflate "untested" surface.** Most `commands/*.rs` files are 0% because `#[tauri::command]` handlers need the Tauri runtime. The underlying repo/business logic they delegate to is often well-tested via `src-tauri/tests/`.
- **happy-dom limitations:** `localStorage.clear`/`removeItem` may be absent; tests use a `shimLocalStorage()` helper. Canvas/sigma/apexcharts must be stubbed in component tests.
- Branch coverage is especially weak; many tests cover the happy path only.