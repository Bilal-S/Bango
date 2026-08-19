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

### `chip-base.vue`

Shared chip scaffold used by `tag-chip.vue` (`variant="filled"`, solid
scheme background) and `label-chip.vue` (`variant="dot"`, transparent with a
leading color dot). Owns the `getColorScheme` bindings, the optional
`highlight` indigo match halo, and the optional `(N)` count suffix; wrappers
forward props and pick a variant (DOM output is byte-identical per variant).
The canonical place for the chip pattern going forward.

### `openalex-search.vue`

OpenAlex Search tab (`/articles` → Search). Input row holds `Search`,
`Clear`, and the LLM-gated **Smart Search** button (reads
`store.smartSearchAvailable`, the canonical `useLlmConfigured()` gate; same
size as Search/Clear). The main query input uses the shared `ClearableInput`
(x-affordance; `flex:1 min-w-0` wrapper so it fills available width). Below
the input row sits a collapsible **SEARCH OPTIONS** box that mirrors the
`article-metadata.vue` "Metadata" box (`border border-slate-200 rounded
overflow-hidden`, label-caps header, `expand_more` caret; click to expand;
shows the active-option count when collapsed, e.g. "2 active"). The panel
body uses **`v-show`** (not `v-if`) so uncommitted panel edits survive a
collapse/re-expand; `panelFilters` is seeded once at setup and re-synced from
the store only while collapsed (so Smart Search still flows in). It surfaces
the `OpenAlexFilters` dimensions the backend already supported but the UI
previously did not: Work Type (chips → `workTypes`; active chip uses the same
accent color as Smart Search), then a single wrapping row holding Publication
Year range (→ `yearFrom`/`yearTo`; letters `e/+/-/.` blocked via keydown),
Language (narrower `<select>` → `language`), Open access only + Include
retracted (switch toggles → `isOa`/`showRetracted`) - all on one level by
default, wrapping only on narrow viewports. Panel edits commit on **Apply**
via `store.setFilters` (auto-re-searches when `hasSearched`); **Clear options**
resets to `DEFAULT_OPENALEX_FILTERS`. All option defaults match the pre-panel
search behavior so the panel is purely additive. The bulk **Add to Working**
button shows `Adding...` + disables while `importSelected` is in flight.

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

**Unsent-input draft persistence**: the store also holds `inputDraft` (the
article/wiki chat `<input>` text) and `citationDraft` (the Citation Finder
`<textarea>` prose). chat-view is NOT keep-alive cached (only `WikiView` and
`ArticleList` are), so the component unmounts on navigation away and any local
input ref would be destroyed. The store-backed drafts are the persistence
mechanism so the user's typed-but-unsent text survives a tab-away + tab-back.
`v-model` binds the two inputs directly to `chatStore.inputDraft` /
`chatStore.citationDraft`. `clearChat()` deliberately does NOT clear these
("Clear Chat" wipes conversation history, not in-progress typing).

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

### Bibliometric network graph quartet (`*-network-graph.vue`)

`citation-network-graph.vue`, `cocitation-network-graph.vue`,
`keyword-network-graph.vue`, and `network-graph.vue` (co-author) share
scaffolding instead of duplicating it:

- `composables/use-network-graph.ts` owns the Sigma renderer lifecycle
  (rAF-deferred init with unmount guards, container via
  `useTemplateRef('sigmaContainer')`), hover-tooltip state, sigma event
  bindings, and the standard reapply watchers (focusedNodeId, colorMode,
  selectedClusters, recalculateTrigger). The co-author graph passes
  `installStandardWatchers: false` plus `onBeforeInit`/`onGraphReady` because
  it dispatches its own focus > cluster > clear logic per prop change.
- `components/graph-status-overlay.vue` renders the loading/error/empty
  chain; only the hover tooltip stays in each domain component.
- `types/network-graph.ts` (`NetworkGraphProps`, `NetworkColorMode`,
  `NetworkSearchSuggestion`) is the shared props contract; domain components
  extend it via `defineProps<NetworkGraphProps & { ... }>`.
- `IsolationDirection` remains exported from `citation-network-graph.vue`
  (imported by the citations view and paper detail panel).
- `composables/use-biblio-network-fetch.ts` backs the network composables
  (`createBiblioNetworkState` module-scope state bundle, `runNetworkFetch`
  loading/error scaffold, `scaleToRange`). The keyword composable keeps its
  bespoke worker flow (loading stays held open until the layout worker
  responds) but shares the state bundle and `scaleToRange`.
- The controls sidebars (`citation/cocitation/keyword/network-controls.vue`)
  are built from `network-search-box.vue` (v-model + input/select/
  select-first/clear emits; Enter-select deliberately skips the filter
  emit in keyword/coauthor - that asymmetry lives in the parents),
  `network-threshold-slider.vue` (input = every tick, commit = release), and
  `network-export-menu.vue` (owns its own open state).

## Work Guidance

- Follow the root `AGENTS.md` User Preferences for the LLM-configured gate.
- Reuse `clearable-input.vue` for text inputs with a clear affordance.
- Keep-alive cached views must use `defineOptions({ name: ... })`.
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
  `openalex-result-item.vue`, `openalex-detail-panel.vue`. Shared network
  primitives: `graph-status-overlay.vue`, `network-search-box.vue`,
  `network-threshold-slider.vue`, `network-export-menu.vue` (see Local
  Contracts above). `help/` holds the
  six `help-tab-*.vue` tab components. `settings/` holds the settings
  sub-components (see Local Contracts above).
- **`composables/`** - Vue composables. Notable: `use-startup-upgrade.ts`,
  `use-bibliometrics.ts`, `use-journal-info.ts`, `use-article-search.ts`,
  `use-network-view.ts`, `use-nav-history.ts`, `use-full-text-attachment.ts`,
  `use-article-delete.ts`, `use-gap-analysis.ts`, `use-llm-configured.ts`,
  `use-wiki.ts`, `use-llm-config.ts`, `use-saved-report.ts` (shared
  saved-report factory behind `use-summary` + `use-gap-analysis`),
  `use-network-graph.ts` + `use-biblio-network-fetch.ts` (shared scaffolding
  behind the four biblio network views; see Local Contracts above).
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