import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('@/composables/use-citation-finder', () => ({
  findCitations: vi.fn(),
  cancelSearch: vi.fn(),
  stopCitationListeners: vi.fn(),
}));

import { useChatStore } from '@/stores/chat';
import { findCitations } from '@/composables/use-citation-finder';
import type { CitationFinderProgress } from '@/types/citation-finder';
import { shimLocalStorage } from '../helpers/fixtures';

const DEFAULTS = { working: true, included: true, rejected: false };
const STORAGE_KEY = 'bango-citation-statuses';
const SOURCE_KEY = 'bango-chat-source';

describe('useChatStore - citation status persistence', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('citation_statuses_default_and_persist', () => {
    const store = useChatStore();
    expect(store.citationStatuses).toEqual(DEFAULTS);

    store.setCitationStatuses({ working: true, included: false, rejected: false });
    expect(store.citationStatuses).toEqual({ working: true, included: false, rejected: false });
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual({
      working: true,
      included: false,
      rejected: false,
    });

    // A fresh store (app restart) reads the persisted selection back.
    setActivePinia(createPinia());
    expect(useChatStore().citationStatuses).toEqual({
      working: true,
      included: false,
      rejected: false,
    });
  });

  it('citation_statuses_invalid_storage_falls_back_to_defaults', () => {
    localStorage.setItem(STORAGE_KEY, '{not json');
    setActivePinia(createPinia());
    expect(useChatStore().citationStatuses).toEqual(DEFAULTS);

    localStorage.setItem(STORAGE_KEY, JSON.stringify({ working: 'yes', included: 1 }));
    setActivePinia(createPinia());
    expect(useChatStore().citationStatuses).toEqual(DEFAULTS);
  });

  it('clears_citation_progress_when_the_search_command_rejects', async () => {
    vi.mocked(findCitations).mockRejectedValueOnce(new Error('boom'));
    const store = useChatStore();
    const progress: CitationFinderProgress = {
      phase: 'searching',
      stage: 'classifying',
      done: 0,
      total: 0,
      overallPercent: 0,
      message: 'Classifying…',
      isRunning: true,
      isCancelled: false,
    };
    store.citationProgress = progress;

    await store.sendCitationSearch('Sugar is bad for you', ['working']);

    expect(store.citationProgress).toBeNull();
  });
});

describe('useChatStore - chat source (mode) persistence', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('chat_source_default_and_persist', () => {
    const store = useChatStore();
    expect(store.source).toBe('articles');

    store.setSource('citation-finder');
    expect(localStorage.getItem(SOURCE_KEY)).toBe('citation-finder');

    /* toggleWikiMode flips to wiki and writes through. */
    store.toggleWikiMode();
    expect(store.source).toBe('wiki');
    expect(localStorage.getItem(SOURCE_KEY)).toBe('wiki');

    // A fresh store (navigation in-session via the singleton, or an app
    // restart reading localStorage) keeps the chosen mode.
    setActivePinia(createPinia());
    expect(useChatStore().source).toBe('wiki');
  });

  it('chat_source_invalid_storage_falls_back_to_articles', () => {
    localStorage.setItem(SOURCE_KEY, 'banana');
    setActivePinia(createPinia());
    expect(useChatStore().source).toBe('articles');

    /* A legacy/empty value behaves like no value at all. */
    localStorage.setItem(SOURCE_KEY, '');
    setActivePinia(createPinia());
    expect(useChatStore().source).toBe('articles');
  });

  it('clearChat resets the source to articles and persists the reset', () => {
    const store = useChatStore();
    store.setSource('wiki');
    store.clearChat();
    expect(store.source).toBe('articles');
    expect(localStorage.getItem(SOURCE_KEY)).toBe('articles');
  });
});
