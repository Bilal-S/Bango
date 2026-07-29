# Test Coverage Report

Coverage baselines captured via `cargo-llvm-cov` (Rust) and `@vitest/coverage-v8` (Vue/TS). Generated 2026-06-17; refreshed 2026-07-29 with the latest full-suite measurements.

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

| Stack | Baseline (initial) | Previous (2026-07-04) | Current (2026-07-29) | Target |
|-------|--------------------|-----------------------|----------------------|--------|
| **Rust** (`src-tauri/`) | 51.93% lines | 65.47% lines | **64.53%** lines | 70% |
| **Vue/TS** (`src/`) | 17.57% lines | 32.22% lines | **40.37%** lines | 70% |

Rust line coverage dipped ~0.94 pp versus the 2026-07-04 snapshot because new untested code
landed since then (notably the `openalex/` module at 0% and several expanded `commands/`
shims). The Rust test suite itself grew: **1,646 tests passing** (+ 12 ignored live tests)
across 114 test binaries. The Vue/TS suite is now **1,530 tests** across **102 files**
(was 1,025 / 76 files).

### Detailed metric breakdown (2026-07-29)

| Stack | Lines | Statements | Branches | Functions |
|---|---|---|---|---|
| **Rust** | 64.53% (30,373 / 47,070) | - | - | 63.02% (1,820 / 2,888) |
| **Vue/TS** | 40.37% (4,187 / 10,369) | 39.67% | 27.03% | 32.9% |

Rust coverage is dominated by well-tested pure-logic modules (parsing, dedup, biblio
networks, wiki engine, screening chunk retrieval). The low function % comes from untested
`#[tauri::command]` shims (see "Remaining gaps" below).

### Wiki + Chat module coverage

These are the modules touched by the Chat-with-Wiki integration. Every module at or above
the 70% target is marked ✅; the command-shim files are low because `#[tauri::command]`
handlers require the Tauri runtime (the underlying logic they delegate to is tested via
the modules below).

**Rust** (`cargo llvm-cov`):

| Module | Lines | % | Status |
|---|---|---|---|
| `wiki/engine.rs` | 1118 | 98.84% | ✅ |
| `wiki/agents_contract.rs` | 51 | 100% | ✅ |
| `wiki/frontmatter.rs` | 519 | 98.27% | ✅ |
| `wiki/fts.rs` | 622 | 91.96% | ✅ |
| `wiki/chat.rs` | 639 | 91.39% | ✅ |
| `wiki/raw_export.rs` | 1597 | 86.91% | ✅ |
| `wiki/storage.rs` | 320 | 86.56% | ✅ |
| `wiki/templates.rs` | 101 | 89.11% | ✅ |
| `wiki/ingest/` (mod, batching, consolidation, authors, synthesis, concepts, sources, methods, slugs) | varies | 85-97% each | ✅ |
| `commands/wiki_cmd/` (mod, status, pages, ingest, raw_files, search_lint, chat, site_export) | varies | 0-45% | ⚠️ thin shims; logic in `wiki/*` |
| `commands/chat.rs` | 119 | 0% | deferred (thin shim; logic in `wiki/chat.rs`) |

**Vue/TS** (`vitest --coverage`):

| File | Lines % | Status |
|---|---|---|
| `utils/wiki-markdown.ts` | 95.5% | ✅ |
| `utils/wiki-site-export.ts` | 30.18% | ⚠️ export-bundle builder needs integration coverage |
| `stores/chat.ts` | 94.11% | ✅ |
| `composables/use-wiki.ts` | 87.65% | ✅ |
| `components/wiki/wiki-page-editor.vue` | 100% | ✅ |
| `components/wiki/wiki-page-viewer.vue` | 58.02% | ⚠️ partial |
| `components/wiki/wiki-toolbar.vue` | 30.8% | ⚠️ gate logic + Lint; handlers deferred |
| `components/wiki/wiki-graph-panel.vue` | 0% | deferred (sigma/canvas; near-zero business logic) |
| `views/chat-view.vue` | 0% | deferred (template wiring; store + composable + children tested) |
| `views/wiki-view.vue` | 0% | deferred (template wiring; store + composable + children tested) |

---

## Progress log (coverage improvement work)

### Rust (51.93% → 65.47% → 64.53%)

Initial baseline work added these test files:
- `src-tauri/tests/models_test.rs` - model `as_str()`/`Display`/`Default` impls (article, criterion, label, tag, llm_config).
- `src-tauri/tests/token_estimation_test.rs` - `estimate_tokens`, `check_context_window`.
- `src-tauri/tests/error_test.rs` - `AppError` variants + `Serialize` impl.
- `src-tauri/tests/llm_config_repo_test.rs` - `get_config`, `save_config`, `has_config`, `get_config_no_decrypt` (all provider variants, key round-trip).
- `src-tauri/tests/summary_repo_test.rs` - `save_summary` upsert, `get_summary`, `clear_summary`.
- `src-tauri/tests/journal_repo_test.rs` - `match_journal` (ISSN/eISSN/name/empty), `resolve_journal_id`, `get_journal_info` (aggregates + exclusion).
- `src-tauri/tests/prisma_svg_test.rs` - `render_prisma_svg` (structure, ongoing phase, exclusion reasons, truncation, XML escaping).
- `src-tauri/tests/import_pipeline_test.rs` - `read_content`, `parse_and_validate` (strict/none), preview building, `filter_excluded`.
- `src-tauri/tests/summary_engine_test.rs` - `generate_summary` (empty, single, batched, trim, error propagation).

Phase 2 additions (2026-07-04): `manual_translate_test.rs` (4 tests), `broken_language_import_test.rs` (5 tests), `auto_translate_full_text_test.rs` (4 tests).

Since then, the test suite grew with new feature coverage (translation engine, batch
import, article delete cascade, metadata editing, audit coalescing, OpenAlex mapping) but
untested production surface grew faster - the net Rust line % dipped slightly
(65.47% → 64.53%).

### Vue/TS (17.57% → 32.22% → 40.37%)

New test infrastructure: `src/__tests__/helpers/fixtures.ts` (`makeArticle()` factory + `shimLocalStorage()` for happy-dom).

Coverage is now spread across all major directories:
- **Utils** (`src/utils/`): **89.59%** aggregate. 18 files at 100%, including all pure helpers (`debounce`, `formatters`, `color`, `graph-filters`, `wiki-markdown`, `platform`, `network-export`, `reference-flatten`, `share-urls`, `citation-analysis`, `ai-summary-groups`, `biblio-links`, `cocitation-label`, both keyboard-navigation helpers).
- **Stores** (`src/stores/`): **86.07%** aggregate. `articles` (96.42%), `audit` (96.87%), `trends-queue` (98.46%), `llm-config` (96.55%), `openalex` (93.65%), `chat` (94.11%) all above target.
- **Composables** (`src/composables/`): **72.91%** aggregate. 14 composables at 100% (including `use-llm-config`, `use-wiki`, `use-demo`, `use-network-view`, `use-nav-history`, `use-toast`, `use-tauri-command`, `use-viewport`, `use-full-text-attachment`, `use-dedup`, `use-import`, `use-dashboard`).
- **Router** (`src/router/index.ts`): 95.45%.
- **Components**: mixed - well-tested leaf components (`clearable-input`, `label-chip`, `tag-chip`, `status-badge`, `confidence-bar`, `screening-progress-bar`, `suggest-input`, `share-dialog` all >= 92%) coexist with untested presentational/graph components.

Test count: 1,025 → 1,530. Test files: 76 → 102.

---

## Remaining gaps (to reach 70%)

### Rust (~6% gap to target: 64.53% → 70%)

The dominant gap is the **`commands/*.rs` shim layer**: 28 files at 0%, totaling **6,703
lines** of untested code. The largest 0% files by line count are:

| File | Lines | Notes |
|---|---|---|
| `commands/summary.rs` | 1289 | Largest single shim |
| `commands/screening.rs` | 669 | Screening orchestration |
| `commands/openalex.rs` | 548 | OpenAlex search/import |
| `commands/articles.rs` | 537 | Article CRUD |
| `commands/criteria.rs` | 439 | Criteria management |
| `commands/wiki_cmd/pages.rs` | 346 | Wiki page commands |
| `openalex/client.rs` | 344 | OpenAlex HTTP client |
| `commands/tags.rs` | 331 | Tag commands |
| `commands/wiki_cmd/ingest.rs` | 314 | Wiki ingest commands |
| `commands/biblio_cmd.rs` | 221 | Bibliometric commands |
| `commands/labels.rs` | 220 | Label commands |
| `commands/translation.rs` | 172 | Translation commands |
| `openalex/reference_harvest.rs` | 161 | Reference harvesting |
| `commands/scraping.rs` | 157 | Citation Chaser |
| `commands/wiki_cmd/raw_files.rs` | 131 | Wiki raw file commands |
| `commands/chat.rs` | 119 | Chat commands |

**Strategy** (documented in `docs/CLAUDE.md` > Coverage Strategy): extract non-trivial
orchestration from command handlers into `pub fn`s accepting `&Connection` (or pure
inputs), test those, keep the command wrapper thin. Files where this is already partially
done (`commands/references.rs` 19.39%, `commands/import.rs` 25.68%, `commands/dedup.rs`
26.68%, `commands/search_strategy.rs` 42.76%) show the pattern works; the remaining shims
should follow the same decomposition.

**Medium-priority files** (1-50% coverage - deepen existing tests):

| File | % | Lines | Notes |
|---|---|---|---|
| `batch_import/mod.rs` | 3.68% | 571 | Runner orchestration |
| `commands/startup.rs` | 3.87% | 155 | Legacy upgrade |
| `commands/wiki_cmd/status.rs` | 4.71% | 276 | Wiki status |
| `commands/export_cmd.rs` | 5.06% | - | Project export |
| `batch_import/translations_phase.rs` | 7.07% | - | Translation phase |
| `translation/worker.rs` | 17.58% | 165 | Translation worker |
| `commands/references.rs` | 19.39% | - | Reference commands |
| `lib.rs` | 19.86% | - | App entry / invoke_handler |
| `commands/import.rs` | 25.68% | - | Import commands |
| `commands/dedup.rs` | 26.68% | - | Dedup commands |
| `screening/llm_client.rs` | 33.33% | 6 | Trait + impl |
| `scraping/browser.rs` | 35.37% | 82 | Headless Chrome |
| `scraping/citation_chaser.rs` | 37.68% | 812 | Citation scraper (live tests `#[ignore]`d) |
| `db/article_repo/translation.rs` | 38.46% | - | Translation repo |
| `commands/search_strategy.rs` | 42.76% | - | Search strategy |
| `commands/wiki_cmd/site_export.rs` | 45.37% | - | Wiki site export |
| `db/reference_repo.rs` | 46.80% | - | Reference repo |
| `summary/engine.rs` | 49.02% | 306 | Summary engine |

**Well-covered reference modules** (50 files >= 90%): `screening/chunk_retrieval.rs`
(99.25%), `wiki/engine.rs` (98.84%), `wiki/frontmatter.rs` (98.27%), `utils/json_repair.rs`
(97.26%), `screening/decision.rs` (97.71%), `screening/evidence.rs` (92.66%),
`translation/engine.rs` (89.55%), `utils/pdf_extract.rs` (86.04%), `ris/cr_parser.rs`
(83.88%), and the full `wiki/ingest/` family.

### Vue/TS (~30% gap to target: 40.37% → 70%)

The gap is concentrated in **views** and **graph/network components**:

**All 15 views remain at or near 0%** (template-heavy; need shallow mount with stubbed children):

| File | Lines | % |
|---|---|---|
| `views/biblio-timeline.vue` | 302 | 0% |
| `views/criteria-editor.vue` | 295 | 0% |
| `views/biblio-authors.vue` | 250 | 0% |
| `views/wiki-view.vue` | 225 | 0% |
| `views/chat-view.vue` | 184 | 0% |
| `views/biblio-cocitations.vue` | 132 | 0% |
| `views/biblio-dashboard.vue` | 125 | 0% |
| `views/biblio-citations.vue` | 119 | 0% |
| `views/biblio-keywords.vue` | 92 | 0% |
| `views/biblio-coauthors.vue` | 58 | 0% |
| `views/dedup-review.vue` | 35 | 0% |
| `views/prisma-diagram.vue` | 34 | 0% |
| `views/help-guide.vue` | 30 | 0% |
| `views/import-ris.vue` | 28 | 0% |
| `views/article-list.vue` | 259 | 4.24% |
| `views/dashboard.vue` | 101 | 41.58% |
| `views/settings-view.vue` | 2 | 0% (thin shell) |

**34 components at 0%** - the largest are presentational/graph-heavy:

| File | Lines | Notes |
|---|---|---|
| `components/wiki/wiki-graph-panel.vue` | 188 | sigma/canvas; near-zero business logic |
| `components/citation-network-graph.vue` | 153 | sigma graph |
| `components/network-graph.vue` | 138 | shared graph wrapper |
| `components/settings/settings-provider-card.vue` | 133 | settings form |
| `components/cocitation-network-graph.vue` | 129 | sigma graph |
| `components/keyword-network-graph.vue` | 126 | sigma graph |
| `components/citation-controls.vue` | 125 | graph filter controls |
| `components/google-trends-widget.vue` | 111 | embed iframe |
| `components/google-trends-panel.vue` | 110 | embed wrapper |
| `components/keyword-controls.vue` | 96 | graph filter controls |
| `components/network-controls.vue` | 86 | shared graph controls |
| `components/help/help-tab-reference.vue` | 80 | static help text |
| `components/cocitation-controls.vue` | 79 | graph filter controls |
| `components/author-detail-panel.vue` | 66 | slide-over panel |
| `components/nav-sidebar.vue` | 59 | nav routing |
| `components/keyword-detail-panel.vue` | 56 | slide-over panel |
| `components/settings/settings-openalex.vue` | 55 | settings form |

**Network/graph composables** need sigma/graphology mocking: `use-sigma-renderer.ts` (0%),
`use-citation-network.ts` (0%), `use-cocitation-network.ts` (0%), `use-keyword-network.ts`
(0%), `use-main-path-worker.ts` (0%), `use-network-layout.ts` (42.85%).

**Medium-priority partial-coverage files** (1-49% - deepen existing tests):

| File | % | Lines |
|---|---|---|
| `views/article-list.vue` | 4.24% | 259 |
| `components/full-text-reader.vue` | 7.46% | 67 |
| `components/bulk-action-bar.vue` | 10% | 10 |
| `components/criteria-edit-dialog.vue` | 10.81% | 37 |
| `components/export-dialog.vue` | 11.42% | 35 |
| `components/article-references.vue` | 14.38% | 139 |
| `components/openalex-search.vue` | 16.66% | 48 |
| `components/reference-paper-detail-panel.vue` | 21.53% | 65 |
| `components/article-toolbar.vue` | 22.72% | 22 |
| `components/references-view.vue` | 24.52% | 106 |
| `utils/wiki-site-export.ts` | 30.18% | 106 |
| `components/wiki/wiki-toolbar.vue` | 30.8% | 211 |
| `composables/use-network-layout.ts` | 42.85% | 35 |
| `components/openalex-result-item.vue` | 42.85% | 7 |

### Highest-ROI next steps (ranked)

1. **Rust `commands/` decomposition** - extract `pub fn`s from the top-10 0% command shims
   (`summary.rs`, `screening.rs`, `openalex.rs`, `articles.rs`, `criteria.rs`,
   `wiki_cmd/pages.rs`, `tags.rs`, `wiki_cmd/ingest.rs`, `biblio_cmd.rs`, `labels.rs`).
   These 10 files account for ~4,269 of the 6,703 untested lines.
2. **Rust `openalex/` module** - the HTTP client + reference harvest (585 lines at 0%) is
   new logic with extractable pure helpers (URL building, response mapping, dedup). High
   value because it is real business logic, not shims.
3. **Vue/TS view shallow-mount tests** - the 7 `biblio-*.vue` views + `criteria-editor.vue`
   + `article-list.vue` deepening. Use the shared helper that mocks `tauriCommand`,
   Pinia, and the router; stub sigma/chart children.
4. **Vue/TS presentational components** - `author-detail-panel`, `keyword-detail-panel`,
   `nav-sidebar`, `settings-provider-card`, `settings-openalex`, `journal-info-card`,
   `search-strategy-card` are high-ROI mounting targets with minimal mocking.
5. **Branch coverage** is weakest (27.03% Vue/TS); many existing tests cover happy paths
   only. Add edge-case + error-path assertions to existing store/composable tests.

---

## Original baseline analysis (for reference)

<details>
<summary>Initial 0%-coverage lists (before improvement work)</summary>

### Rust - Tier 1 (0% line coverage)

**Command shims:** `commands/app_settings.rs`, `articles.rs`, `biblio_cmd.rs`, `chat.rs`, `criteria.rs`, `dedup.rs`, `export_cmd.rs`, `full_text.rs`, `labels.rs`, `llm_config.rs`, `mod.rs`, `prisma.rs`, `scraping.rs`, `screening.rs`, `startup.rs`, `summary.rs`, `tags.rs`, `trends.rs`.

**Business logic (now covered or improved):** `summary/engine.rs` (was 0%, now 49.02%), `prisma/svg.rs` (was 0%, now 100%), `ris/import_pipeline.rs` (was 0%, now 87.93%), `db/summary_repo.rs` (was 0%, now covered), `error.rs` (was 0%, now covered).

**Models (now covered):** `article.rs`, `criterion.rs`, `label.rs`, `tag.rs`, `llm_config.rs` (all were 0%, now covered via `models_test.rs`).

### Vue/TS - Tier 1 (0% coverage)

**Utils (now covered):** `debounce.ts` (was 0%, now 100%), `formatters.ts` (was 36%, now 98.14%), `color.ts` (was 61%, now 94.28%).

**Stores (now covered):** `trends-queue.ts` (was 0%, now 98.63%), `audit.ts` (was 4%, now 97.05%), `criteria.ts` (was 62%, now 84.14%), `llm-config.ts` (was 60%, now 96.87%), `articles.ts` (was 75%, now 96.77%).

**Router (now covered):** `router/index.ts` (was 0%, now 95.45%).

**Composables (now covered):** `use-feature-flags`, `use-viewport`, `use-tauri-command`, `use-toast`, `use-demo`, `use-llm-config` (all were 0%, now 100%).

**Components (now covered):** `confidence-bar`, `screening-stats`, `detail-header`, `screening-progress-bar`, `audit-timeline`, `matched-criteria`, `article-metadata`, `ai-decision-card`, `article-notes` (all were 0%, now 67-100%).

</details>

## Notes / caveats

- **Command shims inflate "untested" surface.** Most `commands/*.rs` files are 0% because `#[tauri::command]` handlers need the Tauri runtime. The underlying repo/business logic they delegate to is often well-tested via `src-tauri/tests/`.
- **happy-dom limitations:** `localStorage.clear`/`removeItem` may be absent; tests use a `shimLocalStorage()` helper. Canvas/sigma/apexcharts must be stubbed in component tests.
- **Branch coverage is especially weak** (27.03% Vue/TS); many tests cover the happy path only.
- **Live/ignored tests** (12 in Rust): the Citation Chaser headless-Chrome tests require Chrome + network + shinyapps.io and are `#[ignore]`d by default; the OpenAlex live tests are similarly gated. They run via `cargo test -- --ignored` in CI-like environments with the right prerequisites.
- **Rust % can dip when new features land.** The 65.47% → 64.53% dip is not a regression in existing tests; it reflects new untested production code (the `openalex/` module + expanded command shims) landing faster than new test coverage. The absolute covered-line count continues to grow.