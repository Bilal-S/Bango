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

  it('starts with empty chat state and article source', () => {
    const store = useChatStore();
    expect(store.messages).toEqual([]);
    expect(store.selectedArticleIds).toEqual([]);
    expect(store.loading).toBe(false);
    expect(store.source).toBe('articles');
    expect(store.wikiReady).toBe(false);
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

  it('clears chat history and resets source to articles', () => {
    const store = useChatStore();
    store.messages.push({
      role: 'user',
      content: 'hello',
      timestamp: '12:00 PM',
    });
    store.setSource('wiki');
    expect(store.source).toBe('wiki');

    store.clearChat();
    expect(store.messages.length).toBe(0);
    expect(store.source).toBe('articles');
  });

  it('sends message via send_chat_message in article mode', async () => {
    const store = useChatStore();

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tauriCommand as any).mockResolvedValueOnce('This is a simulated AI response.');

    store.addSelectedArticle('art-1');

    await store.sendMessage('What is the main finding?');

    expect(store.messages.length).toBe(2);
    expect(store.messages[0]?.role).toBe('user');
    expect(store.messages[0]?.content).toBe('What is the main finding?');
    expect(store.messages[0]?.source).toBe('articles');
    expect(store.messages[1]?.role).toBe('assistant');
    expect(store.messages[1]?.content).toBe('This is a simulated AI response.');
    expect(store.messages[1]?.source).toBe('articles');

    expect(tauriCommand).toHaveBeenCalledWith('send_chat_message', {
      newMessage: 'What is the main finding?',
      history: [],
      articleIds: ['art-1'],
    });
    expect(tauriCommand).not.toHaveBeenCalledWith('wiki_chat', expect.anything());
  });

  it('sends message via wiki_chat in wiki mode and ignores selected articles', async () => {
    const store = useChatStore();

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tauriCommand as any).mockResolvedValueOnce('Based on [[sugar-tax]], the levy worked.');

    // Even with selected articles, wiki mode must not pass them through.
    store.addSelectedArticle('art-1');
    store.setSource('wiki');

    await store.sendMessage('Did the levy work?');

    expect(store.messages.length).toBe(2);
    expect(store.messages[0]?.source).toBe('wiki');
    expect(store.messages[1]?.source).toBe('wiki');
    expect(store.messages[1]?.content).toContain('[[sugar-tax]]');

    expect(tauriCommand).toHaveBeenCalledWith('wiki_chat', {
      question: 'Did the levy work?',
      history: [],
    });
    expect(tauriCommand).not.toHaveBeenCalledWith('send_chat_message', expect.anything());
  });

  it('passes prior messages as history to wiki_chat', async () => {
    const store = useChatStore();
    store.setSource('wiki');

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tauriCommand as any).mockResolvedValueOnce('first answer');
    await store.sendMessage('q1');

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tauriCommand as any).mockResolvedValueOnce('second answer');
    await store.sendMessage('q2');

    // The second call must include the first user + assistant turn as history.
    expect(tauriCommand).toHaveBeenLastCalledWith('wiki_chat', {
      question: 'q2',
      history: [
        { role: 'user', content: 'q1' },
        { role: 'assistant', content: 'first answer' },
      ],
    });
  });

  it('toggleWikiMode flips source both ways and returns the new value', () => {
    const store = useChatStore();
    expect(store.toggleWikiMode()).toBe('wiki');
    expect(store.source).toBe('wiki');
    expect(store.toggleWikiMode()).toBe('articles');
    expect(store.source).toBe('articles');
  });

  it('setWikiReady updates the flag', () => {
    const store = useChatStore();
    store.setWikiReady(true);
    expect(store.wikiReady).toBe(true);
    store.setWikiReady(false);
    expect(store.wikiReady).toBe(false);
  });
});
