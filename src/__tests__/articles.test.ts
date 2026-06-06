import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useArticlesStore } from '@/stores/articles';
import type { Article } from '@/types';

// Mock the tauri-command composable
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockArticles: Article[] = [
  {
    id: '1',
    sequenceId: 1,
    status: 'duplicate',
    title: 'Article 1',
    authors: ['Author A'],
    abstractText: 'Abstract 1',
    screeningError: false,
    keywords: [],
    tags: [],
    labels: [],
    manualOverride: false,
    importSource: 'test.ris',
    importedAt: '2023-01-01',
    changedAt: '2023-01-01',
    screenedAt: null,
    publicationYear: null,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    url: null,
    language: null,
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    referenceType: null,
    date: null,
    authorAddress: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: null,
    aiReasoning: null,
    aiConfidence: null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    fullText: null,
    fullTextAiSummary: null,
  },
  {
    id: '2',
    sequenceId: 2,
    status: 'included',
    title: 'Article 2',
    authors: ['Author B'],
    abstractText: 'Abstract 2',
    screeningError: false,
    keywords: [],
    tags: [],
    labels: [],
    manualOverride: false,
    importSource: 'test.ris',
    importedAt: '2023-01-01',
    changedAt: '2023-01-02',
    screenedAt: '2023-01-02',
    publicationYear: null,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    url: null,
    language: null,
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    referenceType: null,
    date: null,
    authorAddress: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: null,
    aiReasoning: null,
    aiConfidence: null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    fullText: null,
    fullTextAiSummary: null,
  },
];

describe('Articles Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts with empty articles', () => {
    const store = useArticlesStore();
    expect(store.articles).toEqual([]);
    expect(store.initialized).toBe(false);
  });

  it('fetches articles correctly', async () => {
    const store = useArticlesStore();
    vi.mocked(tauriCommand).mockResolvedValue(mockArticles);

    await store.fetchArticles();

    expect(store.articles).toEqual(mockArticles);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
    expect(tauriCommand).toHaveBeenCalledWith('get_articles');
  });

  it('computes byStatus counts correctly', async () => {
    const store = useArticlesStore();
    vi.mocked(tauriCommand).mockResolvedValue(mockArticles);

    await store.fetchArticles();

    expect(store.byStatus).toEqual({
      duplicate: 1,
      working: 0,
      included: 1,
      rejected: 0,
    });
  });

  it('handles fetch errors', async () => {
    const store = useArticlesStore();
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Fetch failed'));

    await store.fetchArticles();

    expect(store.error).toBe('Fetch failed');
    expect(store.loading).toBe(false);
    expect(store.articles).toEqual([]);
  });

  it('invalidates state correctly', async () => {
    const store = useArticlesStore();
    vi.mocked(tauriCommand).mockResolvedValue(mockArticles);

    await store.fetchArticles();
    expect(store.articles.length).toBe(2);

    store.invalidate();
    expect(store.articles).toEqual([]);
    expect(store.initialized).toBe(false);
  });
});
