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
