import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useArticlesStore } from '@/stores/articles';
import type { Article } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { makeArticle as makeBaseArticle } from '../helpers/fixtures';

function makeArticle(id: string, status: Article['status']): Article {
  return makeBaseArticle({
    title: id,
    abstractText: '',
    authors: [],
    publicationYear: null,
    id,
    status,
  });
}

describe('useArticlesStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty', () => {
    const store = useArticlesStore();
    expect(store.articles).toEqual([]);
    expect(store.totalImported).toBe(0);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
    expect(store.initialized).toBe(false);
  });

  it('byStatus counts articles by status', () => {
    const store = useArticlesStore();
    store.articles = [
      makeArticle('1', 'included'),
      makeArticle('2', 'included'),
      makeArticle('3', 'rejected'),
      makeArticle('4', 'working'),
      makeArticle('5', 'duplicate'),
    ];
    expect(store.byStatus.included).toBe(2);
    expect(store.byStatus.rejected).toBe(1);
    expect(store.byStatus.working).toBe(1);
    expect(store.byStatus.duplicate).toBe(1);
  });

  it('fetchArticles loads articles from backend', async () => {
    const mockArticles = [makeArticle('a1', 'included'), makeArticle('a2', 'rejected')];
    vi.mocked(tauriCommand).mockResolvedValue(mockArticles);

    const store = useArticlesStore();
    await store.fetchArticles();

    expect(tauriCommand).toHaveBeenCalledWith('get_articles');
    expect(store.articles).toEqual(mockArticles);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
    expect(store.totalImported).toBe(2);
  });

  it('fetchArticles sets error on failure', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('network down'));

    const store = useArticlesStore();
    await store.fetchArticles();

    expect(store.error).toBe('network down');
    expect(store.loading).toBe(false);
    expect(store.initialized).toBe(false);
  });

  it('fetchArticles handles non-Error exceptions', async () => {
    vi.mocked(tauriCommand).mockRejectedValue('string error');

    const store = useArticlesStore();
    await store.fetchArticles();

    expect(store.error).toBe('string error');
  });

  it('fetchIfNeeded does nothing when initialized', async () => {
    const store = useArticlesStore();
    store.initialized = true;
    await store.fetchIfNeeded();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('invalidate clears state', () => {
    const store = useArticlesStore();
    store.articles = [makeArticle('1', 'included')];
    store.initialized = true;
    store.invalidate();
    expect(store.articles).toEqual([]);
    expect(store.initialized).toBe(false);
  });

  it('refreshArticle updates an existing article in place', async () => {
    const store = useArticlesStore();
    store.articles = [makeArticle('a1', 'working')];
    const updated = makeArticle('a1', 'included');
    vi.mocked(tauriCommand).mockResolvedValue(updated);

    await store.refreshArticle('a1');

    expect(tauriCommand).toHaveBeenCalledWith('get_article', { id: 'a1' });
    expect(store.articles[0]!.status).toBe('included');
  });

  it('refreshArticle ignores unknown article id', async () => {
    const store = useArticlesStore();
    store.articles = [makeArticle('a1', 'working')];
    vi.mocked(tauriCommand).mockResolvedValue(makeArticle('a2', 'included'));

    await store.refreshArticle('nonexistent');
    expect(store.articles[0]!.status).toBe('working');
  });

  it('refreshArticle swallows errors', async () => {
    const store = useArticlesStore();
    store.articles = [makeArticle('a1', 'working')];
    vi.mocked(tauriCommand).mockRejectedValue(new Error('boom'));

    await expect(store.refreshArticle('a1')).resolves.toBeUndefined();
    expect(store.articles[0]!.status).toBe('working');
  });
});
