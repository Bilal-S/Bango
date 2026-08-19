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
- `use-saved-report.ts` is the shared saved-report factory behind
  `use-summary` + `use-gap-analysis`.
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
