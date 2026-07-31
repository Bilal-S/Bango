import { defineStore } from 'pinia';
import { ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { WikiChatMessage } from '@/types/wiki';
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationResult,
  CitationStyle,
} from '@/types/citation-finder';
import {
  findCitations,
  cancelSearch,
  stopCitationListeners,
} from '@/composables/use-citation-finder';

/** Which retrieval source backs the next outgoing message.
 *  Mutually exclusive: `'articles'` routes through `send_chat_message`;
 *  `'wiki'` through `wiki_chat`; `'citation-finder'` through
 *  `find_citations` + the `citation:*` event listeners. */
type ChatSource = 'articles' | 'wiki' | 'citation-finder';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  /** Which source produced / was paired with this message. Lets the UI badge
   * bubbles and route clicks to the right slide-over (wiki vs. article vs.
   * citation cards). */
  source?: ChatSource;
  /** Structured citation results. Present only when `source ===
   *  'citation-finder'` AND `role === 'assistant'`. The bubble template
   *  branches on this: when present it renders `<CitationResultCard>`s,
   *  otherwise it falls back to the Markdown renderer. */
  citations?: CitationResult[];
  /** The citation style captured at submit time + frozen on the bubble so
   *  each bubble renders all its cards with the style that was active when
   *  the search ran (per-bubble, not per-card). Present only on citation-
   *  finder assistant bubbles. */
  citationStyle?: CitationStyle;
}

export const useChatStore = defineStore('chat', () => {
  const selectedArticleIds = ref<string[]>([]);
  const messages = ref<ChatMessage[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  /** Active retrieval source. Mutually exclusive: 'wiki' routes sends through
   *  `wiki_chat` (BM25 FTS5 RAG); 'articles' routes through `send_chat_message`
   *  with the explicit selected-article context; 'citation-finder' routes
   *  through `find_citations` + the `citation:*` event listeners. */
  const source = ref<ChatSource>('articles');

  /** Whether the wiki is available for chat (initialized AND has at least one
   *  page). Populated by `chat-view.vue` from `wiki_get_status`. When false the
   *  wiki toggle is hidden. */
  const wikiReady = ref(false);

  /** Whether the citation finder toggle should be visible. Populated by
   *  `chat-view.vue` from `get_citation_finder_readiness`. When false (e.g.
   *  Anthropic provider, or embeddings not yet enabled) the toggle is hidden. */
  const citationFinderReady = ref(false);

  /** Citation Finder mode (`whole_block` vs `per_statement`). Default
   *  `whole_block`. Persisted only in-memory for the session (not in
   *  app_settings - it's a UI toggle, not a project-wide preference).
   *  Snake_case wire token - matches the Rust enum's
   *  `#[serde(rename_all = "snake_case")]`. */
  const citationFinderMode = ref<CitationFinderMode>('whole_block');

  /** Citation style. Lives ONLY in the citation-finder input area (spec §8.7);
   *  captured at submit time + frozen per-bubble via
   *  `ChatMessage.citationStyle`. Reuses the 5-style LLM-hint list (no second
   *  source of truth, no `@citation-js`). */
  const citationStyle = ref<CitationStyle>('APA');

  /** Live progress snapshot from the `citation:progress` event. Drives the
   *  progress bar + Cancel button in citation-finder mode. */
  const citationProgress = ref<CitationFinderProgress | null>(null);

  /** True while a cancel is in flight (set by `cancelCitationSearch`, cleared
   *  on any terminal event). Drives the "Cancelling…" spinner. The backend
   *  sets the cancel flag; an in-flight LLM call completes naturally (or hits
   *  the orchestrator timeout) before the cancel fires at the next check
   *  point, so this flag covers that wait window. */
  const cancelling = ref(false);

  function addSelectedArticle(id: string) {
    if (!selectedArticleIds.value.includes(id)) {
      selectedArticleIds.value.push(id);
    }
  }

  function removeSelectedArticle(id: string) {
    selectedArticleIds.value = selectedArticleIds.value.filter((val) => val !== id);
  }

  function clearSelectedArticles() {
    selectedArticleIds.value = [];
  }

  function clearChat() {
    messages.value = [];
    error.value = null;
    source.value = 'articles';
    citationProgress.value = null;
    // Tear down any dangling citation:* listeners (e.g. a search was in
    // flight when the user cleared). The next search re-subscribes.
    stopCitationListeners();
  }

  /** Set the active retrieval source. */
  function setSource(next: ChatSource) {
    source.value = next;
  }

  /** Flip between article and wiki retrieval sources. Returns the new value. */
  function toggleWikiMode(): ChatSource {
    source.value = source.value === 'wiki' ? 'articles' : 'wiki';
    return source.value;
  }

  /** Update the wiki readiness flag (drives toggle visibility). */
  function setWikiReady(ready: boolean) {
    wikiReady.value = ready;
  }

  /** Update the citation-finder readiness flag (drives toggle visibility). */
  function setCitationFinderReady(ready: boolean) {
    citationFinderReady.value = ready;
  }

  /** Set the citation-finder mode. */
  function setCitationFinderMode(mode: CitationFinderMode) {
    citationFinderMode.value = mode;
  }

  /** Set the citation style (only meaningful in the citation-finder input). */
  function setCitationStyle(style: CitationStyle) {
    citationStyle.value = style;
  }

  /** Cancel a running citation search. Sets the `cancelling` flag so the UI
   *  can show a spinner; the flag clears when the backend emits
   *  `citation:error "Cancelled"` (or any terminal event). */
  async function cancelCitationSearch() {
    cancelling.value = true;
    try {
      await cancelSearch();
    } catch {
      // Even if the IPC fails, clear the flag so the spinner doesn't stick.
      cancelling.value = false;
    }
  }

  /**
   * Send a citation search. The status filter is REQUIRED (the backend does
   * not apply a default - an empty array returns the "No articles match the
   * selected filters." empty result). The view owns the checkbox state and
   * passes the live array here; `sendMessage` is no longer used for the
   * citation-finder source.
   *
   * Async-event-driven: the command returns immediately with an initial
   * progress snapshot, and the assistant bubble is pushed onto `messages` by
   * the `citation:done` listener (the store stays the single owner of the
   * message list).
   *
   * @param text          The pasted prose to find citations for.
   * @param statusFilter  Article statuses to include (e.g.
   *                      `['working','included']`). The backend filters this
   *                      against the whitelist `['working','included',
   *                      'rejected']`; `duplicate` is always excluded.
   */
  async function sendCitationSearch(text: string, statusFilter: string[]) {
    if (!text.trim()) return;

    const userMsg: ChatMessage = {
      role: 'user',
      content: text,
      timestamp: new Date().toLocaleTimeString(),
      source: 'citation-finder',
    };
    messages.value.push(userMsg);

    loading.value = true;
    error.value = null;
    cancelling.value = false;

    try {
      // Capture the style NOW so the bubble freezes it even if the user
      // changes the <select> mid-search.
      const style = citationStyle.value;
      await findCitations({
        text,
        mode: citationFinderMode.value,
        statusFilter,
        onProgress: (p) => {
          citationProgress.value = p;
        },
        onDone: (results) => {
          citationProgress.value = null;
          cancelling.value = false;
          // Surface "No articles match the selected filters." when every group
          // is empty (the backend returns `[{ claim: null, matches: [] }]` in
          // that case, so a raw `results.length` check would read 1 and report
          // "Found 0").
          const totalMatches = results.reduce((n, r) => n + r.matches.length, 0);
          const summary =
            totalMatches === 0
              ? 'No articles match the selected filters.'
              : `Found ${totalMatches} citation(s).`;
          messages.value.push({
            role: 'assistant',
            content: summary,
            timestamp: new Date().toLocaleTimeString(),
            source: 'citation-finder',
            citations: results,
            citationStyle: style,
          });
        },
        onError: (msg) => {
          citationProgress.value = null;
          cancelling.value = false;
          error.value = msg;
          messages.value.push({
            role: 'assistant',
            content: `Citation search failed: ${msg}`,
            timestamp: new Date().toLocaleTimeString(),
            source: 'citation-finder',
          });
        },
      });
      // The command returns the initial snapshot; the assistant bubble
      // arrives later via the onDone listener. `loading` is cleared in the
      // finally block; the citationProgress ref drives the progress UI
      // during the event-driven wait.
    } catch (e) {
      cancelling.value = false;
      error.value = e instanceof Error ? e.message : String(e);
      messages.value.push({
        role: 'assistant',
        content: `Error: ${error.value}`,
        timestamp: new Date().toLocaleTimeString(),
        source: 'citation-finder',
      });
    } finally {
      loading.value = false;
    }
  }

  /**
   * Send a message through the active source (articles or wiki). This is a
   * synchronous RPC: the assistant response arrives as the awaited return
   * value. The `'citation-finder'` source is handled by the dedicated
   * [`sendCitationSearch`](#sendCitationSearch) method (async-event-driven),
   * so if `sendMessage` is called while the source is citation-finder it
   * forwards to that method with an empty filter (which the backend rejects
   * with the "No articles match" empty result - the view should call
   * `sendCitationSearch` directly instead).
   */
  async function sendMessage(text: string) {
    if (!text.trim()) return;

    // The citation-finder source has its own dedicated sender.
    if (source.value === 'citation-finder') {
      await sendCitationSearch(text, []);
      return;
    }

    const activeSource = source.value;

    // Add user message, tagged with the active source.
    const userMsg: ChatMessage = {
      role: 'user',
      content: text,
      timestamp: new Date().toLocaleTimeString(),
      source: activeSource,
    };
    messages.value.push(userMsg);

    loading.value = true;
    error.value = null;

    try {
      // Map history to backend format (excluding the user's newest message).
      const historyPayload = messages.value.slice(0, -1).map((m) => ({
        role: m.role,
        content: m.content,
      }));

      let response: string;
      if (activeSource === 'wiki') {
        response = await tauriCommand<string>('wiki_chat', {
          question: text,
          history: historyPayload as WikiChatMessage[],
        });
      } else {
        response = await tauriCommand<string>('send_chat_message', {
          articleIds: selectedArticleIds.value,
          history: historyPayload,
          newMessage: text,
        });
      }

      messages.value.push({
        role: 'assistant',
        content: response,
        timestamp: new Date().toLocaleTimeString(),
        source: activeSource,
      });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      messages.value.push({
        role: 'assistant',
        content: `Error: ${error.value}`,
        timestamp: new Date().toLocaleTimeString(),
        source: activeSource,
      });
    } finally {
      loading.value = false;
    }
  }

  return {
    selectedArticleIds,
    messages,
    loading,
    error,
    source,
    wikiReady,
    citationFinderReady,
    citationFinderMode,
    citationStyle,
    citationProgress,
    cancelling,
    addSelectedArticle,
    removeSelectedArticle,
    clearSelectedArticles,
    clearChat,
    setSource,
    toggleWikiMode,
    setWikiReady,
    setCitationFinderReady,
    setCitationFinderMode,
    setCitationStyle,
    cancelCitationSearch,
    sendCitationSearch,
    sendMessage,
  };
});
