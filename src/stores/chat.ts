import { defineStore } from 'pinia';
import { ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { WikiChatMessage } from '@/types/wiki';

/** Which retrieval source backs the next outgoing message. */
export type ChatSource = 'articles' | 'wiki';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  /** Which source produced / was paired with this message. Lets the UI badge
   * bubbles and route clicks to the right slide-over (wiki vs. article). */
  source?: ChatSource;
}

export const useChatStore = defineStore('chat', () => {
  const selectedArticleIds = ref<string[]>([]);
  const messages = ref<ChatMessage[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  /** Active retrieval source. Mutually exclusive: 'wiki' routes sends through
   *  `wiki_chat` (BM25 FTS5 RAG); 'articles' routes through `send_chat_message`
   *  with the explicit selected-article context. */
  const source = ref<ChatSource>('articles');

  /** Whether the wiki is available for chat (initialized AND has at least one
   *  page). Populated by `chat-view.vue` from `wiki_get_status`. When false the
   *  wiki toggle is hidden. */
  const wikiReady = ref(false);

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

  async function sendMessage(text: string) {
    if (!text.trim()) return;

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
    addSelectedArticle,
    removeSelectedArticle,
    clearSelectedArticles,
    clearChat,
    setSource,
    toggleWikiMode,
    setWikiReady,
    sendMessage,
  };
});
