# src/

## Purpose

Vue 3 + TypeScript + Tailwind v4 frontend. All UI: views, reusable components,
composables, Pinia stores, pure utilities, types, router, styles, and the
bundled demo-project asset.

## Ownership

- App entry is `main.ts` (`bootstrap()`), which mounts `App.vue` and registers
  the router.
- `app-shell.vue` owns the global layout + `<keep-alive>` cache.
- Module-specific notes live in the Child DOX Index below.

## Local Contracts

### Keep-alive caching

`article-list.vue` (`/articles`) and `wiki-view.vue` (`/wiki`) are
**keep-alive cached** via
`<keep-alive :include="['WikiView', 'ArticleList']">` in `app-shell.vue` so
their UI state survives navigation away and back. Both components name
themselves via `defineOptions({ name: ... })` (required for the `include`
matcher to find `<script setup>` components).

- `article-list.vue` caches: active status tab, applied filters (panel +
  query), sort column/direction, current page + page size, toolbar search
  text, multi-select set, opened article detail panel + audit trail, and
  fullscreen state. Its `onActivated` (skipped on the first activation via an
  `isFirstActivation` guard) refreshes the underlying data so the view
  reflects changes that happened while away: `search()` re-runs the preserved
  `query` (rows + tab badges update), and the open article detail + audit
  trail are re-fetched. Route deep-link params (`?articleId=…`,
  `?status=…&tags=…`, biblio/tag/label deep-links) override the preserved
  state when they differ (explicit navigation wins). The References and Search
  tabs skip `search()` (their child components own their data) but still
  refresh tab badges.
- The other three `useArticleSearch()` consumers (`wiki-view.vue`,
  `chat-view.vue`, `biblio-citations.vue`) are NOT affected - they keep
  creating fresh per-view composable instances as today.

### Frontend "is the LLM configured?" gate

See the root `AGENTS.md` User Preferences section for the canonical pattern:
EVERY frontend feature gate that depends on "LLM configured" MUST read
`useLlmConfigured()` (from `src/composables/use-llm-configured.ts`), which
wraps `useLlmConfigStore().isConfigured`. No component, view, composable, or
store may hold a local `isLlmConfigured`/`llmConfigured`/`smartSearchAvailable`
ref populated by a one-shot `has_llm_config` IPC call, nor re-derive the
local-provider check from `apiKeyEncrypted`.

### `bulk-action-bar.vue`

`<BulkActionBar>` is the fixed bottom-center multi-select action bar shown
when ≥1 article row is checked. Emits `bulkInclude`/`bulkReject`/
`bulkMoveToWorking`/`bulkAddTag`/`bulkAddLabel`/`bulkAddToChat`/`bulkExport`/
`clearSelection`. The `bulkExport` emit is the **sole entry point for "export
selected"**: `article-list.vue::handleBulkExport` snapshots
`Array.from(selectedIds.value)` and calls `useExport().exportRisForIds`,
distinct from the toolbar Export button + `ExportDialog`.

### `clearable-input.vue`

Reusable text/number input with a built-in clear ("x") affordance pinned to
the right edge. Props: `modelValue`, `placeholder`, `inputClass`, `disabled`,
`type`, `min`/`max`, `title`. Emits `update:modelValue`, `clear` (ONLY when
"x" is clicked), `enter`, `input`/`focus`/`blur`. The canonical place for the
clearable-input pattern going forward.

### Settings cards (`components/settings/`)

`settings-view.vue` consumes: `settings-provider-card.vue` (consolidated AI
Provider box; **Parameters auto-save** debounced 600ms via
`useLlmConfig().scheduleParamSave` - editing Concurrency / Max Context Tokens
/ Request Delay / Temperature triggers a trailing-edge `save_llm_config` so
the orchestrator's `update_settings` takes effect for the next LLM call
without a manual Save button; the parameters-only save path is safe with
respect to embedding capability: `save_llm_config` guards
`reset_embedding_status` behind `embedding_relevant_changed` so a
parameters-only save preserves a known-good `embedding_status`),
`settings-ai-summaries.vue` (3 toggles: auto-generate-summaries
[localStorage `bango-full-text-summaries`], section-summaries [localStorage
`bango-section-summaries`], auto-translate [DB-backed
`app_settings.auto_translate`]),
`settings-screening-preferences.vue`, `settings-storage.vue`,
`settings-reprocessing.vue`, `settings-project-management.vue`,
`settings-notification-history.vue`, `settings-diagnostics.vue`. Shared card
chrome lives in `settings-card-shared.css`.

### `stores/chat.ts`

Pinia chat store. Holds `selectedArticleIds`, `messages`, `loading`, `error`,
plus the retrieval-source state `source: 'articles'|'wiki'` (default
`'articles'`; mutually exclusive) and `wikiReady` (drives the chat-view wiki
toggle visibility). `sendMessage(text)` branches: `source==='wiki'` calls
`wiki_chat`; otherwise calls `send_chat_message`. Each pushed message records
its `source` for bubble rendering. `toggleWikiMode()` flips the source;
`clearChat()` resets it to `'articles'`.

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

- Follow the root `AGENTS.md` User Preferences for the LLM-configured gate.
- Reuse `clearable-input.vue` for text inputs with a clear affordance.
- Keep-alive cached views must use `defineOptions({ name: ... })`.

## Verification

See the root footer: `npm run check:all` (type-check + eslint + prettier +
vitest + `check:test-inventory`) and `npm run test:coverage`
(`@vitest/coverage-v8`, config in `vitest.config.ts`, report at
`coverage/index.html`).

## Child DOX Index

No child `AGENTS.md` files yet under `src/`. The durable boundaries are:

- **`views/`** - page-level views. `biblio-dashboard.vue` is the
  `/bibliometrics` parent; child routes (`coauthors`, `citations`, `keywords`,
  `timeline`, `authors`) render in its `<router-view>`. `biblio-timeline.vue`
  is the Publication Timeline view. `biblio-authors.vue` is the Author
  Productivity Ranking view. `help-guide.vue` is the `/help` shell (tab bar +
  `?tab=`/`#hash` deep-link routing). `chat-view.vue` is the `/chat` route
  (article-RAG chat + wiki-RAG chat via the Wiki toggle).
  `wiki-view.vue` is the `/wiki` route (sidebar + viewer + editor + graph +
  article detail slide-over).
- **`components/`** - reusable components. Notable: `bulk-action-bar.vue`,
  `clearable-input.vue`, `journal-info-card.vue`, `article-detail-panel.vue`,
  `article-filter-panel.vue`, `article-metadata.vue`, `ai-decision-card.vue`,
  `detail-header.vue`, `wiki-toolbar.vue`, `wiki-page-viewer.vue`,
  `wiki-page-editor.vue`, `wiki-graph-panel.vue`, `openalex-search.vue`,
  `openalex-result-item.vue`, `openalex-detail-panel.vue`. `help/` holds the
  six `help-tab-*.vue` tab components. `settings/` holds the settings
  sub-components (see Local Contracts above).
- **`composables/`** - Vue composables. Notable: `use-startup-upgrade.ts`,
  `use-bibliometrics.ts`, `use-journal-info.ts`, `use-article-search.ts`,
  `use-network-view.ts`, `use-nav-history.ts`, `use-full-text-attachment.ts`,
  `use-article-delete.ts`, `use-gap-analysis.ts`, `use-llm-configured.ts`,
  `use-wiki.ts`, `use-llm-config.ts`.
- **`stores/`** - Pinia stores. Notable: `chat.ts`, `openalex.ts`,
  `llm-config.ts`.
- **`utils/`** - pure utilities. Notable: `network-export.ts`, `formatters.ts`,
  `color.ts`, `debounce.ts`, `next-paint.ts`, `reference-flatten.ts`,
  `citation-analysis.ts`, `graph-filters.ts`, `llm-error.ts`,
  `google-trends.ts`, `wiki-markdown.ts`, `wiki-site-export.ts`, `platform.ts`.
- **`types/`** - TypeScript interfaces (incl. `openalex.ts`, `wiki.ts`,
  `index.ts`).
- **`router/`** - route table.
- **`styles/`** - global CSS (`forms.css`, `base.css`, `help-shared.css`,
  `settings-card-shared.css`).
- **`workers/`** - web workers.