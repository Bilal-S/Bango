# src/views/

## Purpose

Page-level views (one per route): Articles, Dashboard, Chat, Wiki, Settings,
Help, the Bibliometrics suite, and Diagnostics.

## Ownership

- All views render inside `app-shell.vue`'s global layout (see
  `src/AGENTS.md`).
- `biblio-dashboard.vue` is the `/bibliometrics` parent; child routes
  (`coauthors`, `citations`, `keywords`, `timeline`, `authors`) render in its
  `<router-view>`.
- `biblio-timeline.vue` is the Publication Timeline view.
- `biblio-authors.vue` is the Author Productivity Ranking view.
- `help-guide.vue` is the `/help` shell (tab bar + `?tab=`/`#hash` deep-link
  routing).
- `chat-view.vue` is the `/chat` route (article-RAG chat + wiki-RAG chat via
  the Wiki toggle; NOT keep-alive cached).
- `wiki-view.vue` is the `/wiki` route (sidebar + viewer + editor + graph +
  article detail slide-over).

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

## Work Guidance

- Keep-alive cached views must use `defineOptions({ name: ... })`.
- Feature gates that depend on "LLM configured" read `useLlmConfigured()`
  (see `src/AGENTS.md` §Local Contracts).

## Verification

See `src/AGENTS.md`: `npm run check:all` + `npm run test:coverage` (view
tests use the shared helpers in `src/__tests__/helpers/`).

## Child DOX Index

No child `AGENTS.md` files; individual views carry no separate local rules.
