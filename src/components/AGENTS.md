# src/components/

## Purpose

Reusable components: shared chips/inputs, article detail + filter panels,
OpenAlex search, settings cards, the bibliometric network graphs + shared
network primitives, and the help tabs.

## Ownership

- Shared network primitives: `graph-status-overlay.vue`,
  `network-search-box.vue`, `network-threshold-slider.vue`,
  `network-export-menu.vue` (see the graph quartet contract below).
- `article-detail-slide-over.vue`: the shared full-article detail slide-over
  for host views that open an article by id without leaving the view
  (`biblio-citations/coauthors/keywords.vue`). Owns the per-instance
  `useArticleSearch` wiring plus the screening/delete/full-text/clear-reasoning
  orchestrators and the `ArticleDetailPanel` template block; exposes
  `open(id)`/`close()` via template ref and emits `opened`/`closed`/
  `toggle-full-screen` so hosts keep their overlay guards (domain panel
  hiding, canvas hidden while fullscreen).
- Cluster thematic analysis pair: `cluster-legend.vue` (shared Louvain
  legend + single-cluster "Analyze" trigger in the heading row between the
  title and the clear-filter icon, matched to its h-6 height; gated by the
  canonical LLM gate passed down as `llmReady`; adopted by
  `network-controls.vue` +
  `keyword-controls.vue`; citation/cocitation controls can adopt it
  unchanged) and `cluster-themes-panel.vue` (slide-over markdown renderer;
  converts only the registered `author:` / `article:` protocols to
  data-attribute spans, escapes raw HTML, renders every other link as plain
  text).
- `help/` holds the six `help-tab-*.vue` tab components.
- `settings/` holds the settings sub-components (see Settings cards below).
- Other notable components: `journal-info-card.vue`,
  `article-detail-panel.vue`, `article-filter-panel.vue`,
  `article-metadata.vue`, `ai-decision-card.vue`, `detail-header.vue`,
  `wiki-toolbar.vue`, `wiki-page-viewer.vue`, `wiki-page-editor.vue`,
  `wiki-graph-panel.vue`, `openalex-result-item.vue`,
  `openalex-detail-panel.vue`.

## Local Contracts

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
parameters-only save preserves a known-good `embedding_status`;
premium-only **Embedding Model override** input under Model Name/API Key:
auto-saves via a 600ms debounced watcher that skips only propagation - it
checks `useEmbeddingSettings().isPersisted(value)`, which `load()`/`save()`
keep in sync with the backend, instead of a "skip the first change" flag
(that flag swallowed the user's only edit when the stored value was empty,
so a pasted model name was never persisted and the field re-appeared blank
after navigating away; regression test:
`src/__tests__/components/settings-provider-card.test.ts`). The load runs
on mount and again when `isPremium` flips true (flags resolve async during
bootstrap); a pending debounced save is flushed on unmount so quick
navigation never loses the edit),
`settings-ai-summaries.vue` (3 toggles: auto-generate-summaries
[localStorage `bango-full-text-summaries`], section-summaries [localStorage
`bango-section-summaries`], auto-translate [DB-backed
`app_settings.auto_translate`]),
`settings-screening-preferences.vue`, `settings-storage.vue`,
`settings-reprocessing.vue`, `settings-project-management.vue`,
`settings-notification-history.vue`, `settings-diagnostics.vue`. Shared card
chrome lives in `settings-card-shared.css`.

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

- Reuse `clearable-input.vue` for text inputs with a clear affordance.
- New chip variants wrap `chip-base.vue`; never re-implement the chip pattern.

## Verification

See `src/AGENTS.md`: `npm run check:all` + `npm run test:coverage`. Graph
component tests import `sigma-renderer-stub.ts` BEFORE the component under
test (see `src/AGENTS.md` §Shared test helpers).

## Child DOX Index

No child `AGENTS.md` files; `settings/` and `help/` carry no separate local
rules.
