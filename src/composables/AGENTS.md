# src/composables/

## Purpose

Vue composables: article search orchestration, keyboard navigation, network
views, LLM config, wiki, dashboard, saved reports, and the startup upgrade.

## Ownership

- `use-article-search.ts` is the root orchestrator (frozen contract below),
  backed by the
  `use-article-{filters,detail,mutations,bulk,route-params,counts,pagination,selection,full-text}`
  sub-composables.
- `use-network-graph.ts` + `use-biblio-network-fetch.ts` are the shared
  scaffolding behind the four biblio network views (full contract in
  `components/AGENTS.md`).
- `use-network-view.ts` owns the shared view state for those views. Its
  `onLayoutModeChange` is a positioning-only relayout (via
  `use-network-layout.ts::applyLayoutPositions`): x/y are rewritten on the
  visible subgraph but cluster assignments survive and `recalculateTrigger`
  is NOT bumped, so cached cluster thematic analyses persist across
  fixed <-> dynamic switches. `onRecalculate` remains the full
  re-cluster + invalidation path (both share `buildVisibleSubgraph`).
- `use-saved-report.ts` is the shared saved-report factory behind
  `use-summary` + `use-gap-analysis`. Its
  `generate({ style, additionalInstructions, targetWordCount })` builds the
  IPC payload (`citationStyle` always; the premium guidance extras trimmed /
  validated then omitted when blank); the per-report guidance refs behind the
  premium AI-Summary cards live in the two composables and reset via
  `onClear`.
- `use-cluster-themes.ts` is the view-facing composable over the
  `cluster-themes` store (cluster thematic analysis on the co-authorship and
  keyword networks). It owns the centralized cache-invalidation watch
  (array-of-getters on `recalculateTrigger` + graph identity) and the
  protocol-registry wiring (`author:` -> focus+locate, `article:` -> the
  hosting view's in-view article detail slide-over). `useThemesPanel()` is
  the panel-state wrapper both views consume (LLM gate, open/cluster
  tracking, analyze/reanalyze/copy actions). Its `copyMarkdown`
  export wraps the clipboard write with a success/error toast
  (search-strategy-card precedent) so a rejected write never surfaces as an
  unhandled promise rejection.
- `use-article-detail-overlay.ts` owns the `ArticleDetailSlideOver` overlay
  guards (show + full-screen refs + handlers) shared by the biblio network
  views.
- `use-dashboard-cta.ts` + `use-dashboard-activity.ts` sit behind
  `use-dashboard`.
- `use-ai-summary.ts` is the AI-summary submission layer: module-level
  singleton listeners for `article-ai-summary-complete`/`-error` (per-article
  toasts + callback registry + `pendingSummaries` drain) plus the request
  APIs - `requestArticleAiSummary` (single article; detail-panel button and
  the attach auto-summarize hook) and `requestBulkArticleAiSummary`
  (BulkActionBar More-menu "AI Summary" action: one submit toast for the
  batch, one fire-and-forget command per article; eligibility filtering
  stays in `article-list.vue::handleBulkAiSummary`).
- Also: `use-startup-upgrade.ts`, `use-bibliometrics.ts`,
  `use-journal-info.ts`, `use-network-view.ts`, `use-nav-history.ts`,
  `use-full-text-attachment.ts`, `use-article-delete.ts`,
  `use-gap-analysis.ts`, `use-wiki.ts`, `use-llm-config.ts`,
  `use-llm-configured.ts` (the canonical LLM gate, `src/AGENTS.md`),
  `use-embedding-settings.ts` (premium embedding-model override load/save;
  `isPersisted` marks backend-known values so auto-save watchers skip
  propagation, never user edits - contract in `components/AGENTS.md`),
  `use-local-embeddings.ts` (the Embeddings card's state: backend selection
  + local component status + install/cancel/verify/remove + the
  `embedding:component` progress listener, one `listen` per scope released
  on dispose; the `backend` ref is MODULE-LEVEL shared state so the
  Embeddings card and the provider card's override field see switches
  immediately; terminal `done`/`error` events clear `installing` and reload
  the status),
  `use-citation-finder-chat.ts` (chat-view's Citation Finder orchestration:
  readiness + backend-aware toggle state/title with the stale-disabled
  self-heal, the submit pipeline - contextual local-embeddings prompt ->
  model-mismatch dialog -> dispatch - both dialogs' handlers (the mismatch
  Regenerate streams live `embedding:progress` into `regeneratingProgress`),
  and the
  store-persisted Articles-to-Search selection (`setCitationStatuses` writes
  through the store and re-checks readiness under the new scope); wired in
  chat-view with deferred self-references for `checkReadiness`/`runSearch`),
  `use-chat-wiki.ts` (wiki availability, page-title + source-metadata maps,
  drift check, wiki-mode toggle, reader nav stack),
  `use-chat-article-context.ts` (candidate article list + context-set
  toggling + selector open state),
  `use-chat-transcript.ts` (scroll-to-bottom + the citation result-arrival
  anchor; the view owns and binds the container ref),
  and `use-demo.ts` (loads `assets/demo-project.bango.json`).

## Local Contracts

### `use-article-search.ts` frozen returned-object shape

The root orchestrator's returned-object shape is a frozen contract - change
internals only, never the shape.

Filtered searches (`isQueryFiltered(query)` true) run `query_articles` and
`count_query_articles` in parallel; `filteredTotal` holds the true match count
and `use-article-pagination` derives `resultCount`/`totalPages`/range display
from it (unfiltered keeps using the store's per-status totals). The page itself
stays capped at the toolbar page size - the pager reaches the remaining
matches.

Toolbar search field prefixes (`use-article-filters.ts::executeToolbarSearch`):
a leading `a:`, `d:`/`doi:`, `j:`, or `y:`/`year:` (case-insensitive; parsed by
`src/utils/toolbar-search.ts::parseToolbarSearch`) routes the text to that
filter field and mirrors it into the panel filter (both-sides sync like the
`?author=` deep-link); plain text lands in `query.search` and keeps
panel-applied filters combined. `clearSearch` reverts the field a prefix search
had set unless the user edited that field afterwards (edits need Apply).
Invalid `y:` values (non-numeric, outside 1850-2100, flipped range) fall back
to the plain search.

### `use-article-list-keyboard.ts`

### `use-zotero.ts` / `use-zotero-export.ts`

Zotero composables. `use-zotero.ts` (import wizard): connection probe
mapped to the four backend states, collections/preview fetches, and the
`zotero-import:progress` listener (unmount cleanup). `use-zotero-export.ts`
(export panel): the same connection gate plus the `zoteroVersion` 10+ gate,
default selection (connector exact-name match -> `lastCollectionKey` ->
none, ambiguous names fall through), DOI-diff preview, the export run, and
the `zotero-export:progress` listener. `use-import.ts` owns the wizard step
machine (`ImportStep` includes `'zotero'`), the key-based exclusion state
(`zoteroCollectionKey`/`zoteroArticleKeys`/`zoteroLibraryVersion`;
`removedIndices` map to `excludedKeys` at confirm), the library-changed
guard return-to-picker, and the Zotero attachment summary on the complete
step.

Articles-view arrow-key shortcuts + their keep-alive listener lifecycle.

## Work Guidance

- New per-view data composables follow the `use-article-*` decomposition
  pattern: one orchestrator + focused sub-composables.
- Multi-source `watch()` must use the array-of-getters form (see
  `src/AGENTS.md` §Work Guidance).

## Verification

See `src/AGENTS.md`: `npm run check:all` + `npm run test:coverage` (e.g.
`src/__tests__/composables/use-llm-config.test.ts`).

## Child DOX Index

No child `AGENTS.md` files.
