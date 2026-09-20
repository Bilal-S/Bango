# src/stores/

## Purpose

Pinia stores: chat, OpenAlex search state, LLM config.

## Ownership

- `llm-config.ts` is the single source of truth for "LLM configured" (see the
  gate contract in `src/AGENTS.md` §Local Contracts).
- `openalex.ts` holds OpenAlex search state; its `smartSearchAvailable` is
  the canonical gate proxy consumed by `openalex-search.vue`.

## Local Contracts

### `chat.ts`

Pinia chat store. Holds `selectedArticleIds`, `messages`, `loading` (send
failures surface as an `Error: ...` assistant bubble; the error ref itself is
store-internal, not part of the public API),
plus the retrieval-source state `source: 'articles'|'wiki'` (mutually
exclusive) and `wikiReady` (drives the chat-view wiki
toggle visibility). `sendMessage(text)` branches: `source==='wiki'` calls
`wiki_chat`; otherwise calls `send_chat_message`. Each pushed message records
its `source` for bubble rendering. `toggleWikiMode()` flips the source;
`clearChat()` resets it to `'articles'`.

**Chat-mode (source) persistence**: `source` is restored from localStorage
(`bango-chat-source`, validated load - any missing/invalid value falls back
to `'articles'`) on store init and written through on every change via a
sync-flush `watch`, so the selected mode survives both navigation (store
singleton) and app restarts. `clearChat()`'s reset to `'articles'` persists
too. A restored `'wiki'` whose wiki became unavailable self-heals in
`use-chat-wiki`'s mount-time status check; a restored `'citation-finder'`
on a blocked provider surfaces the existing disabled-provider banner.

**Unsent-input draft persistence**: the store also holds `inputDraft` (the
article/wiki chat `<input>` text) and `citationDraft` (the Citation Finder
`<textarea>` prose). chat-view is NOT keep-alive cached (only `WikiView` and
`ArticleList` are), so the component unmounts on navigation away and any local
input ref would be destroyed. The store-backed drafts are the persistence
mechanism so the user's typed-but-unsent text survives a tab-away + tab-back.
`v-model` binds the two inputs directly to `chatStore.inputDraft` /
`chatStore.citationDraft`. `clearChat()` deliberately does NOT clear these
("Clear Chat" wipes conversation history, not in-progress typing).

**Articles-to-Search persistence**: `chatStore.citationStatuses` is the
store-backed selection, persisted to `localStorage` (`bango-citation-statuses`)
with a validated load (any parse/shape failure falls back to the
working+included defaults). This stops a remount or restart from silently
widening a run back to the default status union. `setCitationStatuses` writes
through; the chat view calls it via `use-citation-finder-chat`, which also
re-checks readiness under the new scope. `sendCitationSearch`'s command-reject
path clears `citationProgress` (same as the terminal-event handlers).

## Work Guidance

- Cross-view UI state that must survive component unmount belongs in a store,
  not component-local refs (see the draft-persistence pattern in `chat.ts`).
- `cluster-themes.ts` is the session-only cache for cluster thematic
  analysis: a keyed map (`networkType:clusterIndex` -> `{ markdown, loading,
  error }`) with stale-result dropping; invalidated wholesale by
  `use-cluster-themes.ts` on every recalculate because Louvain indices are
  not stable across runs. `analyze()` short-circuits on the session cache: a
  resolved entry redisplays without a new LLM call and an in-flight duplicate
  is skipped (errored entries retry); the panel's re-analyze is the explicit
  refresh path (deletes the entry first). Stale-result dropping uses a
  per-key generation token: a write-back requires the entry to still be
  loading AND the generation to match, so a late response is dropped whether
  its entry was invalidated or invalidated-and-replaced. No persistence.

## Verification

See `src/AGENTS.md`: `npm run check:all` + `npm run test:coverage`.

## Child DOX Index

No child `AGENTS.md` files.
