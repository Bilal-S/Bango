# src/

## Purpose

Vue 3 + TypeScript + Tailwind v4 frontend. All UI: views, reusable components,
composables, Pinia stores, pure utilities, types, router, styles, and the
bundled demo-project asset.

## Ownership

- App entry is `main.ts` (`bootstrap()`), which mounts `App.vue` and registers
  the router.
- `app-shell.vue` owns the global layout + `<keep-alive>` cache (contract in
  `views/AGENTS.md`).
- This doc owns the cross-cutting frontend contracts (LLM-configured gate,
  multi-source `watch()` rule, shared test helpers); per-module contracts
  live in the Child DOX Index below.

## Local Contracts

### Frontend "is the LLM configured?" gate - single canonical pattern

EVERY frontend feature gate that depends on "LLM configured" (Chat, Wiki,
Screening, OpenAlex Smart Search, Dashboard CTA, AI Summary, AI buttons in
the article detail panel, Search Strategy Builder, Citation Finder
readiness) MUST read `useLlmConfigured()` (from
`src/composables/use-llm-configured.ts`), which wraps
`useLlmConfigStore().isConfigured`.

- No component, view, composable, or store may hold a local
  `isLlmConfigured`/`llmConfigured`/`smartSearchAvailable` ref populated by a
  one-shot `has_llm_config` IPC call, nor re-derive the local-provider
  (`ollama`/`lmStudio`/`llamaCpp`) check from `apiKeyEncrypted`. Both
  patterns go stale on Settings edits (the original bug: clearing the API key
  in Settings did not disable Chat/Wiki/Screening until a manual refresh).
- The Pinia store (`src/stores/llm-config.ts`) is the single source of truth.
  Its `isConfigured` computed mirrors the backend
  `llm_config_repo::has_config` contract (initialized + endpoint + model +
  (local-provider OR API key)). `useLlmConfigStore` keeps the
  `LOCAL_PROVIDERS` set store-private and exports only the
  `isLocalProvider(provider)` predicate, so the local-provider set has exactly
  one frontend definition (mirrors the backend Rust `is_local` match).
- The `has_llm_config` Tauri command stays registered (the screening
  `get_screening_readiness` composite still calls `has_config` server-side),
  but NO frontend caller may invoke it directly. The one exception is
  `screening-progress.vue`, which ANDs the backend composite
  `readiness.hasLlmConfig` with `useLlmConfigured()` so the Start button +
  guardrails react instantly to Settings edits without waiting for the
  composite readiness to re-fetch.
- `use-llm-config.ts::save()` re-fetches the store after every successful
  `save_llm_config` so the in-memory `config` reflects the post-save DB state
  (the backend encrypts `api_key_encrypted`, replacing the plaintext the user
  typed with the encrypted blob). This keeps `isConfigured` accurate after
  every save.

### `styles/forms.css`

Global form/button/dialog primitives (`.field__*`, `.btn--*`, `.dialog`,
`.dialog__danger-box`, `.dialog__info-box`, `.spinner`). Loaded via `base.css`;
low specificity so scoped rules in other views still win.

### `router/index.ts`

Route table; lazy views are prefetched after `router.isReady()`. `/settings`
renders `settings-view.vue`.

### `assets/demo-project.bango.json`

Bundled demo project (loaded as raw text via `?raw` by
`src/composables/use-demo.ts` and passed to `import_project_backup`). Contains
25 articles (11 included, 1 rejected, 2 duplicate, 11 working) spanning
2015-2025, with populated `articleTags`/`articleLabels` junction tables. The
two key UK SDIL papers (Gressier 2025, Dickson 2025) plus 7 additional real UK
sugar-levy studies (Cobiac, Rogers, Pell, Bandy, Amies-Cull, Gillieson) form
the included corpus that powers all six bibliometric tools.
`referencePapers`/`articleReferenceLinks` are left empty for the user to
populate via reference/citation imports. `scripts/enrich_demo.py` is the
idempotent generator (deterministic UUID5 article IDs); re-run after schema
changes.

## Work Guidance

- Follow the LLM-configured gate contract above (recorded as a durable
  preference in the root `AGENTS.md` User Preferences).
- Reuse `clearable-input.vue` for text inputs with a clear affordance (see
  `components/AGENTS.md`).
- Keep-alive cached views must use `defineOptions({ name: ... })` (see
  `views/AGENTS.md`).
- **Multi-source `watch()` MUST use the array-of-getters form**, never a
  getter that returns a fresh array/object. `watch(() => [a.value, b.value],
  cb)` returns a new array reference on every reactive touch, so Vue's
  `Object.is` change check ALWAYS reports "changed" - even when the
  underlying values are identical (e.g. when `store.fetch()` reassigns a ref
  to a new object with the same fields). This caused an infinite
  save -> fetch -> watcher-fires -> save loop in the LLM Settings card
  (status pill flickered forever between "Saving..." and "Not Tested" after a
  single edit). The correct form is `watch([() => a.value, () => b.value],
  cb)`, which compares each element independently and fires only on real
  value changes. Tested in
  `src/__tests__/composables/use-llm-config.test.ts`
  (`clearTestResult reactivity (regression: infinite save loop)`).

## Verification

See the root footer: `npm run check:all` (type-check + eslint + prettier +
vitest + `check:test-inventory`) and `npm run test:coverage`
(`@vitest/coverage-v8`, config in `vitest.config.ts`, report at
`coverage/index.html`).

### Shared test helpers (`src/__tests__/helpers/`)

- `fixtures.ts` - the single canonical `makeArticle` factory (all fields,
  translation fields included; per-file tests add thin aliases carrying only
  their semantic defaults) plus `makeArticlesStore` / `makeTagsStore` /
  `makeLabelsStore` mock-store factories and `shimLocalStorage`. New tests
  MUST NOT re-declare a local full-field `makeArticle` clone.
- `sigma-renderer-stub.ts` - the shared `use-sigma-renderer` WebGL stub for
  graph component tests; import it BEFORE the component under test so the
  `vi.mock` registration lands first. The `sigmaEvents` map is the seam tests
  drive (clear it in `beforeEach`).

## Child DOX Index

Child `AGENTS.md` files exist for the module directories below; the remaining
directories are indexed inline.

- **`views/`** - page-level views (bibliometrics suite, help, chat, wiki,
  articles, settings, dashboard, diagnostics) + the keep-alive caching
  contract. See `views/AGENTS.md`.
- **`components/`** - reusable components: shared chips/inputs
  (`chip-base.vue`, `clearable-input.vue`, `bulk-action-bar.vue`), article
  detail + filter panels, OpenAlex search, settings cards, the bibliometric
  network graph quartet + shared network primitives, help tabs. See
  `components/AGENTS.md`.
- **`composables/`** - Vue composables: the frozen `use-article-search`
  orchestrator + its `use-article-*` sub-composables, keyboard navigation,
  network views, dashboard/saved-report factories, LLM config. See
  `composables/AGENTS.md`.
- **`stores/`** - Pinia stores: `chat.ts` (incl. draft persistence),
  `openalex.ts`, `llm-config.ts` (the gate's single source of truth). See
  `stores/AGENTS.md`.
- **`utils/`** - pure utilities. Notable: `network-export.ts`, `formatters.ts`,
  `color.ts`, `debounce.ts`, `next-paint.ts`, `reference-flatten.ts`,
  `citation-analysis.ts`, `graph-filters.ts`, `llm-error.ts`,
  `google-trends.ts`, `wiki-markdown.ts`, `wiki-site-export.ts`,
  `platform.ts`, `article-keyboard-navigation.ts`, `article-deep-links.ts`
  (pure `parseArticleRouteQuery` for Articles-view deep-links).
- **`types/`** - TypeScript interfaces (incl. `openalex.ts`, `wiki.ts`,
  `index.ts`).
- **`router/`** - route table (see Local Contracts).
- **`styles/`** - global CSS (`forms.css`, `base.css`, `help-shared.css`,
  `settings-card-shared.css`; see Local Contracts).
- **`workers/`** - web workers.
