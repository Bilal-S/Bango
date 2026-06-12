import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useChatStore } from '@/stores/chat';
import { tauriCommand } from '@/composables/use-tauri-command';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

describe('useChatStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts with empty chat state', () => {
    const store = useChatStore();
    expect(store.messages).toEqual([]);
    expect(store.selectedArticleIds).toEqual([]);
    expect(store.loading).toBe(false);
  });

  it('manages selectedArticleIds', () => {
    const store = useChatStore();
    store.addSelectedArticle('art-1');
    expect(store.selectedArticleIds).toEqual(['art-1']);

    store.addSelectedArticle('art-1');
    expect(store.selectedArticleIds).toEqual(['art-1']);

    store.addSelectedArticle('art-2');
    expect(store.selectedArticleIds).toEqual(['art-1', 'art-2']);

    store.removeSelectedArticle('art-1');
    expect(store.selectedArticleIds).toEqual(['art-2']);

    store.clearSelectedArticles();
    expect(store.selectedArticleIds).toEqual([]);
  });

  it('clears chat history', () => {
    const store = useChatStore();
    store.messages.push({
      role: 'user',
      content: 'hello',
      timestamp: '12:00 PM',
    });
    expect(store.messages.length).toBe(1);

    store.clearChat();
    expect(store.messages.length).toBe(0);
  });

  it('sends message and updates conversation history', async () => {
    const store = useChatStore();

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tauriCommand as any).mockResolvedValueOnce('This is a simulated AI response.');

    store.addSelectedArticle('art-1');

    await store.sendMessage('What is the main finding?');

    expect(store.messages.length).toBe(2);
    expect(store.messages[0]?.role).toBe('user');
    expect(store.messages[0]?.content).toBe('What is the main finding?');
    expect(store.messages[1]?.role).toBe('assistant');
    expect(store.messages[1]?.content).toBe('This is a simulated AI response.');

    expect(tauriCommand).toHaveBeenCalledWith('send_chat_message', {
      newMessage: 'What is the main finding?',
      history: [],
      articleIds: ['art-1'],
    });
  });
});
