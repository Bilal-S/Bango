import { defineStore } from 'pinia';
import { ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { WikiChatMessage } from '@/types/wiki';
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationFinderReadiness,
  CitationResult,
  CitationStatusFlags,
  CitationStyle,
} from '@/types/citation-finder';
import {
  findCitations,
  cancelSearch,
  stopCitationListeners,
} from '@/composables/use-citation-finder';

/** Retrieval source for next outgoing message. Mutually exclusive. */
type ChatSource = 'articles' | 'wiki' | 'citation-finder';

/** localStorage key for the persisted Articles-to-Search selection. */
const CITATION_STATUSES_STORAGE_KEY = 'bango-citation-statuses';

/** Working + Included default ON, Rejected default OFF; duplicates excluded. */
const DEFAULT_CITATION_STATUSES: CitationStatusFlags = {
  working: true,
  included: true,
  rejected: false,
};

/**
 * Read the persisted Articles-to-Search selection. Any missing key, parse
 * error, or non-boolean shape falls back to the defaults (never throws).
 * @returns A fresh flags object safe to mutate.
 */
function loadCitationStatuses(): CitationStatusFlags {
  try {
    const raw = localStorage.getItem(CITATION_STATUSES_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_CITATION_STATUSES };
    const parsed = JSON.parse(raw) as Partial<Record<keyof CitationStatusFlags, unknown>>;
    if (
      typeof parsed?.working !== 'boolean' ||
      typeof parsed?.included !== 'boolean' ||
      typeof parsed?.rejected !== 'boolean'
    ) {
      return { ...DEFAULT_CITATION_STATUSES };
    }
    return { working: parsed.working, included: parsed.included, rejected: parsed.rejected };
  } catch {
    return { ...DEFAULT_CITATION_STATUSES };
  }
}

export interface ChatMessage {
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

  /* Unsent chat input drafts. chat-view is NOT keep-alive cached (only
   * WikiView + ArticleList are), so the component unmounts on navigation
   * away. The store is the persistence mechanism for typed-but-unsent text
   * so the user does not lose their draft when they tab out and back. The
   * two drafts mirror the two distinct inputs in chat-view: the
   * article/wiki single-line <input> + the citation-finder <textarea>.
   * `clearChat()` deliberately does NOT clear these (Clear Chat wipes the
   * conversation history, not in-progress typing). */
  const inputDraft = ref('');
  const citationDraft = ref('');

  /** Wiki available for chat (initialized AND has pages). */
  const wikiReady = ref(false);

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

  /** Articles-to-Search selection. Persisted so navigation/restart does not
   *  silently widen a run back to the default working+included union. */
  const citationStatuses = ref<CitationStatusFlags>(loadCitationStatuses());

  /** Live progress from `citation:progress` event. */
  const citationProgress = ref<CitationFinderProgress | null>(null);

  /** True while a cancel is in flight (set by `cancelCitationSearch`, cleared
   *  on any terminal event). Drives the "Cancelling…" spinner; the backend
   *  aborts the in-flight call and emits `citation:error "Cancelled"`. */
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

  /** Set full readiness payload (drives toggle state via `citationToggleState`). */
  function setCitationReadiness(r: CitationFinderReadiness | null) {
    citationReadiness.value = r;
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

  /** Persist the Articles-to-Search selection (best-effort localStorage). */
  function setCitationStatuses(next: CitationStatusFlags) {
    citationStatuses.value = { ...next };
    try {
      localStorage.setItem(CITATION_STATUSES_STORAGE_KEY, JSON.stringify(citationStatuses.value));
    } catch {
      // Best-effort: the in-memory selection still applies this session.
    }
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
      citationProgress.value = null;
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
    source,
    inputDraft,
    citationDraft,
    wikiReady,
    citationReadiness,
    mismatchDismissedFor,
    citationFinderMode,
    citationStyle,
    citationStatuses,
    citationProgress,
    cancelling,
    addSelectedArticle,
    removeSelectedArticle,
    clearSelectedArticles,
    clearChat,
    setSource,
    toggleWikiMode,
    setWikiReady,
    setCitationReadiness,
    setMismatchDismissed,
    setCitationFinderMode,
    setCitationStyle,
    setCitationStatuses,
    cancelCitationSearch,
    sendCitationSearch,
    sendMessage,
  };
});
