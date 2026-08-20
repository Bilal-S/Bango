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
  `use-summary` + `use-gap-analysis`.
- `use-cluster-themes.ts` is the view-facing composable over the
  `cluster-themes` store (cluster thematic analysis on the co-authorship and
  keyword networks). It owns the centralized cache-invalidation watch
  (array-of-getters on `recalculateTrigger` + graph identity) and the
  protocol-registry wiring (`author:` -> focus+locate, `article:` -> the
  hosting view's in-view article detail slide-over). Its `copyMarkdown`
  export wraps the clipboard write with a success/error toast
  (search-strategy-card precedent) so a rejected write never surfaces as an
  unhandled promise rejection.
- `use-dashboard-cta.ts` + `use-dashboard-activity.ts` sit behind
  `use-dashboard`.
- Also: `use-startup-upgrade.ts`, `use-bibliometrics.ts`,
  `use-journal-info.ts`, `use-network-view.ts`, `use-nav-history.ts`,
  `use-full-text-attachment.ts`, `use-article-delete.ts`,
  `use-gap-analysis.ts`, `use-wiki.ts`, `use-llm-config.ts`,
  `use-llm-configured.ts` (the canonical LLM gate, `src/AGENTS.md`), and
  `use-demo.ts` (loads `assets/demo-project.bango.json`).

## Local Contracts

### `use-article-search.ts` frozen returned-object shape

The root orchestrator's returned-object shape is a frozen contract - change
internals only, never the shape.

### `use-article-list-keyboard.ts`

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
