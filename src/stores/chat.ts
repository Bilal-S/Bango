import { defineStore } from 'pinia';
import { ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
}

export const useChatStore = defineStore('chat', () => {
  const selectedArticleIds = ref<string[]>([]);
  const messages = ref<ChatMessage[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

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
  }

  async function sendMessage(text: string) {
    if (!text.trim()) return;

    // Add user message
    const userMsg: ChatMessage = {
      role: 'user',
      content: text,
      timestamp: new Date().toLocaleTimeString(),
    };
    messages.value.push(userMsg);

    loading.value = true;
    error.value = null;

    try {
      // Map history to backend format (excluding user's newest message)
      const historyPayload = messages.value.slice(0, -1).map((m) => ({
        role: m.role,
        content: m.content,
      }));

      const response = await tauriCommand<string>('send_chat_message', {
        articleIds: selectedArticleIds.value,
        history: historyPayload,
        newMessage: text,
      });

      // Add assistant message
      messages.value.push({
        role: 'assistant',
        content: response,
        timestamp: new Date().toLocaleTimeString(),
      });
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      messages.value.push({
        role: 'assistant',
        content: `Error: ${error.value}`,
        timestamp: new Date().toLocaleTimeString(),
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
    addSelectedArticle,
    removeSelectedArticle,
    clearSelectedArticles,
    clearChat,
    sendMessage,
  };
});
