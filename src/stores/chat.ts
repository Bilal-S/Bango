import { defineStore } from 'pinia';
import { ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { WikiChatMessage } from '@/types/wiki';
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationFinderReadiness,
  CitationResult,
  CitationStyle,
} from '@/types/citation-finder';
import {
  findCitations,
  cancelSearch,
  stopCitationListeners,
} from '@/composables/use-citation-finder';

/** Retrieval source for next outgoing message. Mutually exclusive. */
type ChatSource = 'articles' | 'wiki' | 'citation-finder';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  /** Which source produced this message. Lets UI route clicks to right slide-over. */
  source?: ChatSource;
  /** Structured citation results. Present only on citation-finder assistant messages. */
  citations?: CitationResult[];
  /** Citation style captured at submit time, frozen per bubble. */
  citationStyle?: CitationStyle;
}

export const useChatStore = defineStore('chat', () => {
  const selectedArticleIds = ref<string[]>([]);
  const messages = ref<ChatMessage[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  /** Active retrieval source. Mutually exclusive. */
  const source = ref<ChatSource>('articles');

  /** Wiki available for chat (initialized AND has pages). */
  const wikiReady = ref(false);

  /** Citation finder toggle visibility. @deprecated Use `citationReadiness` +
   *  `citationToggleState` instead. Kept for backward-compat. */
  const citationFinderReady = ref(false);

  /** Full readiness payload (drives toggle state via `citationToggleState`).
   *  `null` until first IPC completes. Refreshed reactively on LLM config
   *  changes via view watcher. */
  const citationReadiness = ref<CitationFinderReadiness | null>(null);

  /** Stored-model key for dismissed model-mismatch dialog. `null` = no dismissal. */
  const mismatchDismissedFor = ref<string | null>(null);

  /** Citation Finder mode. Session-scoped, not persisted. */
  const citationFinderMode = ref<CitationFinderMode>('whole_block');

  /** Citation style. Captured at submit, frozen per-bubble. */
  const citationStyle = ref<CitationStyle>('APA');

  /** Live progress from `citation:progress` event. */
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
    // Reset the mismatch-dismissal tracker so a new session gets a fresh
    // chance to warn the user if their embeddings are stale relative to the
    // current model.
    mismatchDismissedFor.value = null;
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

  /** Set full readiness payload. Also updates legacy `citationFinderReady` for backward-compat. */
  function setCitationReadiness(r: CitationFinderReadiness | null) {
    citationReadiness.value = r;
    // Mirror the derived bool so the welcome card's `citationFinderReady`
    // branch keeps working without a separate watcher.
    citationFinderReady.value = r ? r.providerSupportsEmbeddings : false;
  }

  /** Record that the user dismissed the model-mismatch dialog for the given
   *  stored-model key, so subsequent searches in the same session do not nag.
   *  Pass `null` to reset (e.g. after a regenerate completes). */
  function setMismatchDismissed(storedModel: string | null) {
    mismatchDismissedFor.value = storedModel;
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
   * Send a citation search. Async-event-driven: command returns initial
   * snapshot, assistant bubble pushed by `citation:done` listener.
   * @param statusFilter Required; backend whitelist is
   *   `['working','included','rejected']` (duplicate always excluded).
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
   * Send message through active source (articles or wiki). Synchronous RPC.
   * Citation-finder source forwards to `sendCitationSearch` (async-event-driven).
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
    citationReadiness,
    mismatchDismissedFor,
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
    setCitationReadiness,
    setMismatchDismissed,
    setCitationFinderMode,
    setCitationStyle,
    cancelCitationSearch,
    sendCitationSearch,
    sendMessage,
  };
});
