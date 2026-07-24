import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import type { Article, AuditEntry, ArticleCounts } from '@/types';

// ── Mock tauri-command ──────────────────────────────────────────────
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

// ── Mock stores ─────────────────────────────────────────────────────
const mockArticlesStore = {
  byStatus: { duplicate: 0, working: 5, included: 3, rejected: 2 },
  totalImported: 10,
};

const mockTagsStore = {
  tags: [
    {
      id: 't1',
      name: 'machine-learning',
      source: 'user_created' as const,
      color: null,
      articleCount: 5,
    },
    { id: 't2', name: 'nlp', source: 'ai_suggested' as const, color: null, articleCount: 3 },
  ],
  fetchTags: vi.fn(),
};

const mockLabelsStore = {
  labels: [
    {
      id: 'l1',
      name: 'priority-read',
      source: 'user_created' as const,
      color: null,
      articleCount: 2,
    },
    { id: 'l2', name: 'disputed', source: 'ai_generated' as const, color: null, articleCount: 1 },
  ],
  fetchLabels: vi.fn(),
};

vi.mock('@/stores/articles', () => ({
  useArticlesStore: vi.fn(() => mockArticlesStore),
}));

vi.mock('@/stores/tags', () => ({
  useTagsStore: vi.fn(() => mockTagsStore),
}));

vi.mock('@/stores/labels', () => ({
  useLabelsStore: vi.fn(() => mockLabelsStore),
}));

// ── Imports (after mocks) ───────────────────────────────────────────
import { tauriCommand } from '@/composables/use-tauri-command';
import { useArticleSearch } from '@/composables/use-article-search';

// ── Test fixtures ───────────────────────────────────────────────────
function makeArticle(overrides: Partial<Article> & { id: string }): Article {
  return {
    id: overrides.id,
    sequenceId: overrides.sequenceId ?? 1,
    status: overrides.status ?? 'working',
    screeningError: overrides.screeningError ?? false,
    title: overrides.title ?? 'Test Article',
    abstractText: overrides.abstractText ?? 'Abstract text',
    authors: overrides.authors ?? ['Author A'],
    publicationYear: overrides.publicationYear ?? 2024,
    doi: overrides.doi ?? null,
    journal: overrides.journal ?? null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: overrides.keywords ?? [],
    url: null,
    language: null,
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    journalIndexId: null,
    referenceType: null,
    date: null,
    authorAddress: null,
    affiliation: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: overrides.aiDecision ?? null,
    aiReasoning: overrides.aiReasoning ?? null,
    aiConfidence: overrides.aiConfidence ?? null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    tags: overrides.tags ?? [],
    labels: overrides.labels ?? [],
    manualOverride: false,
    importSource: 'test.ris',
    importedAt: '2024-01-01T00:00:00Z',
    changedAt: overrides.changedAt ?? '2024-01-01T00:00:00Z',
    screenedAt: overrides.screenedAt ?? null,
    fullText: null,
    fullTextAiSummary: null,
    numCited: null,
    numReferences: null,
    hasCitationDetails: false,
    hasReferenceDetails: false,
    hasFullText: false,
    fullTextFileName: null,
    hasFiguresOrTables: false,
    isTranslated: false,
    translationStatus: 'none',
    translationError: null,
    translatedAt: null,
  };
}

const sampleArticles: Article[] = [
  makeArticle({ id: 'a1', sequenceId: 1, title: 'Alpha', authors: ['Alice'], status: 'working' }),
  makeArticle({
    id: 'a2',
    sequenceId: 2,
    title: 'Beta',
    authors: ['Bob', 'Carol'],
    status: 'included',
    aiConfidence: 0.85,
    screenedAt: '2024-01-02',
  }),
  makeArticle({
    id: 'a3',
    sequenceId: 3,
    title: 'Gamma',
    authors: ['Dave'],
    status: 'rejected',
    aiConfidence: 0.3,
  }),
];

const sampleCounts: ArticleCounts = {
  all: 10,
  duplicate: 0,
  working: 5,
  included: 3,
  rejected: 2,
  error: 0,
  references: 0,
};

// ── Helpers ─────────────────────────────────────────────────────────
/** Configure tauriCommand mock to return articles + counts on search. */
function mockSearchResults(articles = sampleArticles, counts = sampleCounts) {
  vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
    if (cmd === 'query_articles') return Promise.resolve(articles);
    if (cmd === 'get_article_counts') return Promise.resolve(counts);
    return Promise.resolve(undefined);
  });
}

// ── Test suites ─────────────────────────────────────────────────────

describe('useArticleSearch', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    // Reset store mock state
    mockArticlesStore.byStatus = { duplicate: 0, working: 5, included: 3, rejected: 2 };
    mockArticlesStore.totalImported = 10;
  });

  // ── Initial State ──────────────────────────────────────────────────
  describe('initial state', () => {
    it('defaults to working tab when working articles exist', () => {
      const { activeStatusTab } = useArticleSearch();
      expect(activeStatusTab.value).toBe('working');
    });

    it('defaults to included tab when no working but included articles exist', () => {
      mockArticlesStore.byStatus = { duplicate: 0, working: 0, included: 3, rejected: 2 };
      const { activeStatusTab } = useArticleSearch();
      expect(activeStatusTab.value).toBe('included');
    });

    it('defaults to all tab when no working or included articles', () => {
      mockArticlesStore.byStatus = { duplicate: 0, working: 0, included: 0, rejected: 0 };
      const { activeStatusTab } = useArticleSearch();
      expect(activeStatusTab.value).toBe('all');
    });

    it('seeds statusCounts from store', () => {
      const { statusCounts } = useArticleSearch();
      expect(statusCounts.value.working).toBe(5);
      expect(statusCounts.value.included).toBe(3);
    });

    it('starts with empty articles', () => {
      const { articles } = useArticleSearch();
      expect(articles.value).toEqual([]);
    });

    it('starts with loading false', () => {
      const { loading } = useArticleSearch();
      expect(loading.value).toBe(false);
    });

    it('starts with page size 10', () => {
      const { pageSize } = useArticleSearch();
      expect(pageSize.value).toBe(10);
    });
  });

  // ── Search ─────────────────────────────────────────────────────────
  describe('search()', () => {
    it('calls query_articles with the current query', async () => {
      mockSearchResults();
      const { search } = useArticleSearch();
      await search();
      expect(tauriCommand).toHaveBeenCalledWith('query_articles', expect.any(Object));
    });

    it('populates articles from the response', async () => {
      mockSearchResults();
      const { search, articles } = useArticleSearch();
      await search();
      expect(articles.value).toEqual(sampleArticles);
    });

    it('sets loading to true during fetch and false after', async () => {
      let resolveSearch!: (v: Article[]) => void;
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'query_articles')
          return new Promise<Article[]>((r) => {
            resolveSearch = r;
          });
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        return Promise.resolve(undefined);
      });

      const { search, loading } = useArticleSearch();
      const p = search();
      expect(loading.value).toBe(true);
      resolveSearch(sampleArticles);
      await p;
      expect(loading.value).toBe(false);
    });

    it('sets error on failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('DB error'));
      const { search, error } = useArticleSearch();
      await search();
      expect(error.value).toBe('DB error');
    });

    it('fetches counts after search', async () => {
      mockSearchResults();
      const { search } = useArticleSearch();
      await search();
      expect(tauriCommand).toHaveBeenCalledWith('get_article_counts', {});
    });
  });

  // ── Toolbar Search ─────────────────────────────────────────────────
  describe('toolbar search', () => {
    it('executeToolbarSearch sets query.search and calls search', async () => {
      mockSearchResults();
      const { searchText, executeToolbarSearch } = useArticleSearch();
      searchText.value = 'alpha';
      executeToolbarSearch();
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ search: 'alpha' }),
          })
        );
      });
    });

    it('executeToolbarSearch resets to page 1', async () => {
      mockSearchResults();
      const { currentPage, searchText, executeToolbarSearch, goToPage } = useArticleSearch();
      // Simulate being on page 2
      goToPage(2);
      await vi.waitFor(() => expect(currentPage.value).toBe(2));

      searchText.value = 'test';
      executeToolbarSearch();
      expect(currentPage.value).toBe(1);
    });

    it('clearSearch clears text and query', async () => {
      mockSearchResults();
      const { searchText, clearSearch } = useArticleSearch();
      searchText.value = 'something';
      clearSearch();
      expect(searchText.value).toBe('');
    });

    it('clearSearch sets query.search to null', async () => {
      mockSearchResults();
      const { searchText, executeToolbarSearch, clearSearch } = useArticleSearch();
      searchText.value = 'test';
      executeToolbarSearch();
      clearSearch();
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenLastCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ search: null }),
          })
        );
      });
    });
  });

  // ── Sort ───────────────────────────────────────────────────────────
  describe('toggleSort()', () => {
    it('sets sort column and defaults to asc', async () => {
      mockSearchResults();
      const { toggleSort, sortColumn, sortDirection } = useArticleSearch();
      toggleSort('title');
      expect(sortColumn.value).toBe('title');
      expect(sortDirection.value).toBe('asc');
    });

    it('toggles direction when same column', async () => {
      mockSearchResults();
      const { toggleSort, sortDirection } = useArticleSearch();
      toggleSort('title');
      expect(sortDirection.value).toBe('asc');
      toggleSort('title');
      expect(sortDirection.value).toBe('desc');
    });

    it('resets to asc when switching columns', async () => {
      mockSearchResults();
      const { toggleSort, sortColumn, sortDirection } = useArticleSearch();
      toggleSort('title');
      toggleSort('title'); // now desc
      toggleSort('publicationYear'); // new column → asc
      expect(sortColumn.value).toBe('publicationYear');
      expect(sortDirection.value).toBe('asc');
    });

    it('triggers a search with updated sort params', async () => {
      mockSearchResults();
      const { toggleSort } = useArticleSearch();
      toggleSort('title');
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ sortBy: 'title', sortDir: 'asc' }),
          })
        );
      });
    });
  });

  // ── Filters ────────────────────────────────────────────────────────
  describe('filters', () => {
    it('applyFilters updates query params from filter state', async () => {
      mockSearchResults();
      const s = useArticleSearch();
      s.filter.yearFrom = 2020;
      s.filter.yearTo = 2024;
      s.filter.authorText = 'Alice';
      s.filter.tags = ['ml'];
      s.filter.labels = ['priority'];
      s.applyFilters();
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({
              yearFrom: 2020,
              yearTo: 2024,
              author: 'Alice',
              tags: ['ml'],
              labels: ['priority'],
            }),
          })
        );
      });
    });

    it('applyFilters forwards excludedTags/excludedLabels to the query', async () => {
      mockSearchResults();
      const s = useArticleSearch();
      s.filter.excludedTags = ['nlp'];
      s.filter.excludedLabels = ['disputed'];
      s.applyFilters();
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({
              excludedTags: ['nlp'],
              excludedLabels: ['disputed'],
            }),
          })
        );
      });
    });

    it('isFiltered is true when only excludedTags are set', async () => {
      mockSearchResults();
      const s = useArticleSearch();
      // Baseline: no filters
      expect(s.isFiltered.value).toBe(false);
      s.applyFilters();
      s.filter.excludedTags = ['old-topic'];
      s.applyFilters();
      await vi.waitFor(() => {
        expect(s.isFiltered.value).toBe(true);
      });
    });

    it('applyFilters resets to page 1', async () => {
      mockSearchResults();
      const { applyFilters, currentPage, goToPage } = useArticleSearch();
      goToPage(3);
      await vi.waitFor(() => expect(currentPage.value).toBe(3));
      applyFilters();
      expect(currentPage.value).toBe(1);
    });

    it('clearFilters resets all filter fields', async () => {
      mockSearchResults();
      const s = useArticleSearch();
      s.filter.yearFrom = 2020;
      s.filter.tags = ['ml'];
      s.filter.excludedTags = ['old'];
      s.filter.excludedLabels = ['dropped'];
      s.clearFilters();
      expect(s.filter.yearFrom).toBeNull();
      expect(s.filter.tags).toEqual([]);
      expect(s.filter.excludedTags).toEqual([]);
      expect(s.filter.excludedLabels).toEqual([]);
    });

    it('clearFilters resets query params to defaults', async () => {
      mockSearchResults();
      const s = useArticleSearch();
      s.filter.authorText = 'Bob';
      s.applyFilters();
      s.clearFilters();
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenLastCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({
              author: null,
              tags: [],
              labels: [],
              excludedTags: [],
              excludedLabels: [],
            }),
          })
        );
      });
    });

    it('toggleFilters toggles showFilters', () => {
      const { showFilters, toggleFilters } = useArticleSearch();
      expect(showFilters.value).toBe(false);
      toggleFilters();
      expect(showFilters.value).toBe(true);
      toggleFilters();
      expect(showFilters.value).toBe(false);
    });
  });

  // ── Status Tabs ────────────────────────────────────────────────────
  describe('setStatusTab()', () => {
    it('sets the active tab and queries with the status', async () => {
      mockSearchResults();
      const { setStatusTab, activeStatusTab } = useArticleSearch();
      setStatusTab('included');
      expect(activeStatusTab.value).toBe('included');
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ status: 'included' }),
          })
        );
      });
    });

    it('sets status to null for "all" tab', async () => {
      mockSearchResults();
      const { setStatusTab } = useArticleSearch();
      setStatusTab('all');
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ status: null }),
          })
        );
      });
    });

    it('maps "error" tab to working + screeningErrorsOnly', async () => {
      mockSearchResults();
      const { setStatusTab } = useArticleSearch();
      setStatusTab('error');
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ status: 'working', screeningErrorsOnly: true }),
          })
        );
      });
    });

    it('resets to page 1 on tab change', async () => {
      mockSearchResults();
      const { setStatusTab, currentPage, goToPage } = useArticleSearch();
      goToPage(2);
      await vi.waitFor(() => expect(currentPage.value).toBe(2));
      setStatusTab('rejected');
      expect(currentPage.value).toBe(1);
    });
  });

  // ── Pagination ─────────────────────────────────────────────────────
  describe('pagination', () => {
    it('computes totalPages from activeTotalCount and pageSize', async () => {
      mockSearchResults();
      const { search, totalPages, setStatusTab } = useArticleSearch();
      setStatusTab('all');
      await search();
      // activeTotalCount for "all" = 10 (from mock store), pageSize = 10 → 1 page
      expect(totalPages.value).toBe(1);
    });

    it('computes totalPages = ceil(count / pageSize)', async () => {
      // 25 working articles, page size 10 → 3 pages
      mockArticlesStore.byStatus.working = 25;
      mockSearchResults(sampleArticles, { ...sampleCounts, working: 25 });
      const { search, totalPages } = useArticleSearch();
      await search();
      // The composable defaults to "working" tab, activeTotalCount reads from statusCounts
      // After fetchCounts, statusCounts.working = 25
      expect(totalPages.value).toBe(3);
    });

    // ── Filtered pagination (regression: page count must track the filtered
    //    result length, NOT the unfiltered tab total) ──────────────────
    it('totalPages is 1 when filtered even if activeTotalCount is larger', async () => {
      // Tab total is 25, but the filtered query returns only 2 articles.
      mockArticlesStore.byStatus.working = 25;
      const filteredArticles = [sampleArticles[0]!, sampleArticles[1]!];
      mockSearchResults(filteredArticles, { ...sampleCounts, working: 25 });
      const s = useArticleSearch();
      s.filter.tags = ['some-tag'];
      s.applyFilters();
      await vi.waitFor(() => expect(s.isFiltered.value).toBe(true));
      // resultCount = filteredArticles.length = 2; ceil(2/10) = 1 page.
      expect(s.resultCount.value).toBe(2);
      expect(s.totalPages.value).toBe(1);
    });

    it('selectedGlobalIndex is the 1-based position within the filtered page', async () => {
      // Tab total is 25, but the filtered query returns only 2 articles.
      mockArticlesStore.byStatus.working = 25;
      const filteredArticles = [sampleArticles[0]!, sampleArticles[1]!];
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'query_articles') return Promise.resolve(filteredArticles);
        if (cmd === 'get_article_counts') return Promise.resolve({ ...sampleCounts, working: 25 });
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return Promise.resolve(filteredArticles.find((a) => a.id === id) ?? filteredArticles[0]);
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });
      const s = useArticleSearch();
      s.filter.tags = ['some-tag'];
      s.applyFilters();
      await vi.waitFor(() => expect(s.isFiltered.value).toBe(true));
      // Select the first filtered article (index 0 → 1-based position 1).
      await s.selectArticle(filteredArticles[0]!.id);
      expect(s.selectedGlobalIndex.value).toBe(1);
      // Selecting the second filtered article yields position 2 (no offset math).
      await s.selectArticle(filteredArticles[1]!.id);
      expect(s.selectedGlobalIndex.value).toBe(2);
    });

    it('goToPage updates currentPage and offset', async () => {
      mockArticlesStore.byStatus.working = 25;
      mockSearchResults();
      const { goToPage, currentPage } = useArticleSearch();
      goToPage(2);
      expect(currentPage.value).toBe(2);
      await vi.waitFor(() => {
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({ offset: 10 }),
          })
        );
      });
    });

    it('canGoPrev is false on page 1', () => {
      const { canGoPrev } = useArticleSearch();
      expect(canGoPrev.value).toBe(false);
    });

    it('canGoNext is false on last page', async () => {
      mockSearchResults();
      const { search, canGoNext } = useArticleSearch();
      await search();
      // 10 articles, page size 10 → 1 page → canGoNext = false
      expect(canGoNext.value).toBe(false);
    });

    it('changePageSize updates pageSize and resets to page 1', async () => {
      mockSearchResults();
      const { changePageSize, pageSize, currentPage, goToPage } = useArticleSearch();
      goToPage(2);
      await vi.waitFor(() => expect(currentPage.value).toBe(2));
      changePageSize(25);
      expect(pageSize.value).toBe(25);
      expect(currentPage.value).toBe(1);
    });

    it('rangeStart is 1-based index of first displayed article', async () => {
      mockArticlesStore.byStatus.working = 25;
      mockSearchResults();
      const { search, rangeStart } = useArticleSearch();
      await search();
      expect(rangeStart.value).toBe(1);
    });

    it('rangeEnd is 1-based index of last displayed article on page', async () => {
      mockSearchResults();
      const { search, rangeEnd } = useArticleSearch();
      await search();
      // rangeEnd = min(page * pageSize, activeTotalCount) = min(10, 5) = 5
      // activeTotalCount comes from statusCounts, which was seeded from store (working=5)
      expect(rangeEnd.value).toBe(5);
    });

    it('rangeStart is 0 when no results', async () => {
      // rangeStart uses resultCount, which falls back to activeTotalCount when not filtered.
      // We need both empty articles AND zero counts to get rangeStart=0
      mockArticlesStore.byStatus = { duplicate: 0, working: 0, included: 0, rejected: 0 };
      mockSearchResults([], {
        all: 0,
        duplicate: 0,
        working: 0,
        included: 0,
        rejected: 0,
        error: 0,
        references: 0,
      });
      const { search, rangeStart, setStatusTab } = useArticleSearch();
      setStatusTab('all');
      await search();
      expect(rangeStart.value).toBe(0);
    });
  });

  // ── Multi-Select ───────────────────────────────────────────────────
  describe('multi-select', () => {
    it('toggleSelect adds an id', () => {
      const { toggleSelect, selectedIds } = useArticleSearch();
      toggleSelect('a1');
      expect(selectedIds.value.has('a1')).toBe(true);
    });

    it('toggleSelect removes an existing id', () => {
      const { toggleSelect, selectedIds } = useArticleSearch();
      toggleSelect('a1');
      toggleSelect('a1');
      expect(selectedIds.value.has('a1')).toBe(false);
    });

    it('toggleSelectAll selects all articles when not all selected', async () => {
      mockSearchResults();
      const { search, toggleSelectAll, selectedIds } = useArticleSearch();
      await search();
      toggleSelectAll();
      expect(selectedIds.value.size).toBe(sampleArticles.length);
    });

    it('toggleSelectAll deselects when all are already selected', async () => {
      mockSearchResults();
      const { search, toggleSelectAll, selectedIds } = useArticleSearch();
      await search();
      toggleSelectAll(); // select all
      toggleSelectAll(); // deselect all
      expect(selectedIds.value.size).toBe(0);
    });

    it('clearSelection empties the set', () => {
      const { toggleSelect, clearSelection, selectedIds } = useArticleSearch();
      toggleSelect('a1');
      toggleSelect('a2');
      clearSelection();
      expect(selectedIds.value.size).toBe(0);
    });

    // ── Range selection (shift-click) ─────────────────────────────
    it('toggleSelectRange without shift acts like toggleSelect', async () => {
      mockSearchResults();
      const { search, toggleSelectRange, selectedIds } = useArticleSearch();
      await search();
      toggleSelectRange('a1', false);
      expect(selectedIds.value.has('a1')).toBe(true);
    });

    it('toggleSelectRange with shift selects range from last toggled', async () => {
      mockSearchResults();
      const { search, toggleSelectRange, selectedIds } = useArticleSearch();
      await search();
      // First click sets anchor
      toggleSelectRange('a1', false);
      // Shift-click selects a1..a3 inclusive
      toggleSelectRange('a3', true);
      expect(selectedIds.value.has('a1')).toBe(true);
      expect(selectedIds.value.has('a2')).toBe(true);
      expect(selectedIds.value.has('a3')).toBe(true);
      expect(selectedIds.value.size).toBe(3);
    });

    it('toggleSelectRange works backwards', async () => {
      mockSearchResults();
      const { search, toggleSelectRange, selectedIds } = useArticleSearch();
      await search();
      toggleSelectRange('a3', false);
      toggleSelectRange('a1', true);
      expect(selectedIds.value.has('a1')).toBe(true);
      expect(selectedIds.value.has('a2')).toBe(true);
      expect(selectedIds.value.has('a3')).toBe(true);
    });

    it('consecutive shift-clicks extend from original anchor', async () => {
      mockSearchResults();
      const { search, toggleSelectRange, selectedIds } = useArticleSearch();
      await search();
      toggleSelectRange('a1', false);
      toggleSelectRange('a2', true);
      // Next shift-click extends from original anchor a1 to a3
      toggleSelectRange('a3', true);
      expect(selectedIds.value.has('a1')).toBe(true);
      expect(selectedIds.value.has('a2')).toBe(true);
      expect(selectedIds.value.has('a3')).toBe(true);
      expect(selectedIds.value.size).toBe(3);
    });

    it('shift-click with no prior anchor falls back to toggle', async () => {
      mockSearchResults();
      const { search, toggleSelectRange, selectedIds } = useArticleSearch();
      await search();
      toggleSelectRange('a2', true);
      expect(selectedIds.value.size).toBe(1);
      expect(selectedIds.value.has('a2')).toBe(true);
    });

    it('selectedCount returns size of selected set', () => {
      const { toggleSelect, selectedCount } = useArticleSearch();
      expect(selectedCount.value).toBe(0);
      toggleSelect('a1');
      expect(selectedCount.value).toBe(1);
    });

    it('allSelected is true when all articles are selected', async () => {
      mockSearchResults();
      const { search, toggleSelect, allSelected } = useArticleSearch();
      await search();
      for (const a of sampleArticles) toggleSelect(a.id);
      expect(allSelected.value).toBe(true);
    });

    it('allSelected is false when no articles', () => {
      const { allSelected } = useArticleSearch();
      expect(allSelected.value).toBe(false);
    });

    it('someSelected is true when partial selection', async () => {
      mockSearchResults();
      const { search, toggleSelect, someSelected } = useArticleSearch();
      await search();
      toggleSelect('a1');
      expect(someSelected.value).toBe(true);
    });

    it('someSelected is false when all selected', async () => {
      mockSearchResults();
      const { search, toggleSelectAll, someSelected } = useArticleSearch();
      await search();
      toggleSelectAll();
      expect(someSelected.value).toBe(false);
    });
  });

  // ── Bulk Operations ────────────────────────────────────────────────
  describe('bulk operations', () => {
    it('bulkUpdateStatus calls Tauri command and clears selection', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);
      mockSearchResults();
      const { search, toggleSelect, bulkUpdateStatus, selectedIds } = useArticleSearch();
      await search();
      toggleSelect('a1');
      toggleSelect('a2');
      await bulkUpdateStatus(['a1', 'a2'], 'included');
      expect(tauriCommand).toHaveBeenCalledWith('bulk_update_article_status', {
        ids: ['a1', 'a2'],
        newStatus: 'included',
      });
      expect(selectedIds.value.size).toBe(0);
    });

    it('bulkAddTag calls Tauri command and refreshes', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);
      const { bulkAddTag } = useArticleSearch();
      await bulkAddTag(['a1'], 'ml');
      expect(tauriCommand).toHaveBeenCalledWith('bulk_add_tag_to_articles', {
        articleIds: ['a1'],
        tagName: 'ml',
      });
      expect(mockTagsStore.fetchTags).toHaveBeenCalled();
    });

    it('bulkAddLabel calls Tauri command and refreshes', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);
      const { bulkAddLabel } = useArticleSearch();
      await bulkAddLabel(['a1'], 'priority');
      expect(tauriCommand).toHaveBeenCalledWith('bulk_add_label_to_articles', {
        articleIds: ['a1'],
        labelName: 'priority',
      });
      expect(mockLabelsStore.fetchLabels).toHaveBeenCalled();
    });
  });

  // ── Article Detail ─────────────────────────────────────────────────
  describe('article detail', () => {
    it('selectArticle fetches article and audit trail', async () => {
      const article = sampleArticles[0]!;
      const auditEntries: AuditEntry[] = [
        {
          id: 'au1',
          articleId: 'a1',
          timestamp: '2024-01-01',
          action: 'import',
          fromStatus: null,
          toStatus: 'working',
          details: 'Imported',
          source: 'system',
          articleTitle: 'Alpha',
        },
      ];
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'get_article') return Promise.resolve(article);
        if (cmd === 'get_audit_trail') return Promise.resolve(auditEntries);
        return Promise.resolve(undefined);
      });

      const { selectArticle, selectedArticle, auditTrail, showDetail } = useArticleSearch();
      await selectArticle('a1');
      expect(selectedArticle.value).toEqual(article);
      expect(auditTrail.value).toEqual(auditEntries);
      expect(showDetail.value).toBe(true);
    });

    it('closeDetail hides detail panel and clears selection', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { selectArticle, closeDetail, showDetail, selectedArticle } = useArticleSearch();
      await selectArticle('a1');
      expect(showDetail.value).toBe(true);
      closeDetail();
      expect(showDetail.value).toBe(false);
      expect(selectedArticle.value).toBeNull();
    });

    it('closeDetail navigates back to return target article', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return Promise.resolve({ ...sampleArticles[0], id });
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { selectArticle, navigateToArticle, closeDetail, selectedArticle } = useArticleSearch();
      await selectArticle('a1');
      await navigateToArticle('a2');
      expect(selectedArticle.value?.id).toBe('a2');
      closeDetail();
      // closeDetail() fires selectArticle(returnId) as void (fire-and-forget)
      // Need to wait for the async selectArticle to complete
      await vi.waitFor(() => {
        expect(selectedArticle.value?.id).toBe('a1');
      });
    });
  });

  // ── refreshArticle ─────────────────────────────────────────────────
  describe('refreshArticle', () => {
    it('fetches the article, patches the articles list row, and fetches counts', async () => {
      // Simulate a screening decision: article was `working`, refresh returns
      // `included` with a screenedAt timestamp.
      const fresh = {
        ...sampleArticles[0]!,
        status: 'included' as const,
        screenedAt: '2024-02-03',
        aiConfidence: 0.92,
      };
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'query_articles') return Promise.resolve(sampleArticles);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return id === 'a1' ? Promise.resolve(fresh) : Promise.resolve(sampleArticles[0]);
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { search, selectArticle, refreshArticle, articles, selectedArticle } =
        useArticleSearch();
      await search();
      await selectArticle('a1');

      // Pre-condition: the table row shows the old status
      expect(articles.value.find((a) => a.id === 'a1')?.status).toBe('working');

      await refreshArticle('a1');

      // selectedArticle reflects the fresh fetch
      expect(selectedArticle.value?.status).toBe('included');
      expect(selectedArticle.value?.screenedAt).toBe('2024-02-03');

      // The articles list row is patched (so the table color bar updates)
      expect(articles.value.find((a) => a.id === 'a1')?.status).toBe('included');
      expect(articles.value.find((a) => a.id === 'a1')?.screenedAt).toBe('2024-02-03');

      // Counts were fetched (tab badges refresh)
      expect(tauriCommand).toHaveBeenCalledWith('get_article_counts', {});
    });

    it('is a no-op for the articles list when the article is not in the list', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'query_articles') return Promise.resolve(sampleArticles);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return Promise.resolve({ ...sampleArticles[0]!, id, status: 'included' as const });
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { search, refreshArticle, articles, selectedArticle } = useArticleSearch();
      await search();

      // Refresh an article id that is NOT in the current list - syncArticleToList
      // should find no index and leave the list unchanged.
      await refreshArticle('nonexistent');
      expect(selectedArticle.value?.id).toBe('nonexistent');
      // The list is unchanged (still the original sampleArticles)
      expect(articles.value).toEqual(sampleArticles);
    });
  });

  // ── Navigation ─────────────────────────────────────────────────────
  describe('navigatePrev / navigateNext', () => {
    it('navigateNext selects the next article on the same page', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'query_articles') return Promise.resolve(sampleArticles);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return Promise.resolve(sampleArticles.find((a) => a.id === id) ?? sampleArticles[0]);
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { search, selectArticle, navigateNext, selectedArticle } = useArticleSearch();
      await search();
      await selectArticle('a1');
      await navigateNext();
      expect(selectedArticle.value?.id).toBe('a2');
    });

    it('navigatePrev selects the previous article on the same page', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'query_articles') return Promise.resolve(sampleArticles);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return Promise.resolve(sampleArticles.find((a) => a.id === id) ?? sampleArticles[0]);
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { search, selectArticle, navigatePrev, selectedArticle } = useArticleSearch();
      await search();
      await selectArticle('a3');
      await navigatePrev();
      expect(selectedArticle.value?.id).toBe('a2');
    });

    it('hasPrevious is false when at the start of page 1', async () => {
      mockSearchResults();
      const { search, selectArticle, hasPrevious } = useArticleSearch();
      await search();
      await selectArticle('a1');
      expect(hasPrevious.value).toBe(false);
    });

    it('hasNext is false when at the end of the last page', async () => {
      mockSearchResults();
      const { search, selectArticle, hasNext } = useArticleSearch();
      await search();
      await selectArticle('a3');
      expect(hasNext.value).toBe(false);
    });
  });

  // ── Full Text ──────────────────────────────────────────────────────
  describe('full text operations', () => {
    it('attachFullText calls Tauri command and refreshes article', async () => {
      const updated = { ...sampleArticles[0], hasFullText: true, fullTextFileName: 'paper.pdf' };
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'attach_full_text') return Promise.resolve(undefined);
        if (cmd === 'get_article') return Promise.resolve(updated);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        return Promise.resolve(undefined);
      });

      const { attachFullText } = useArticleSearch();
      await attachFullText('a1', '/path/to/paper.pdf');
      expect(tauriCommand).toHaveBeenCalledWith('attach_full_text', {
        articleId: 'a1',
        filePath: '/path/to/paper.pdf',
      });
    });

    it('deleteFullTextAttachment calls Tauri command', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'delete_full_text') return Promise.resolve(undefined);
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        return Promise.resolve(undefined);
      });

      const { deleteFullTextAttachment } = useArticleSearch();
      await deleteFullTextAttachment('a1');
      expect(tauriCommand).toHaveBeenCalledWith('delete_full_text', { articleId: 'a1' });
    });

    it('readFullTextContent calls Tauri command and returns content', async () => {
      vi.mocked(tauriCommand).mockResolvedValue('Full text content here');

      const { readFullTextContent } = useArticleSearch();
      const result = await readFullTextContent('a1');
      expect(tauriCommand).toHaveBeenCalledWith('read_full_text', { articleId: 'a1' });
      expect(result).toBe('Full text content here');
    });
  });

  // ── Computed Properties ────────────────────────────────────────────
  describe('computed properties', () => {
    it('allAuthors extracts unique sorted authors from articles', async () => {
      mockSearchResults();
      const { search, allAuthors } = useArticleSearch();
      await search();
      expect(allAuthors.value).toEqual(['Alice', 'Bob', 'Carol', 'Dave']);
    });

    it('allTags returns sorted tag names from the tags store', () => {
      const { allTags } = useArticleSearch();
      expect(allTags.value).toEqual(['machine-learning', 'nlp']);
    });

    it('allLabels returns sorted label names from the labels store', () => {
      const { allLabels } = useArticleSearch();
      expect(allLabels.value).toEqual(['disputed', 'priority-read']);
    });

    it('isFiltered is false when no filters active', async () => {
      mockSearchResults();
      const { search, isFiltered } = useArticleSearch();
      await search();
      expect(isFiltered.value).toBe(false);
    });

    it('isFiltered is true when search text is set', async () => {
      mockSearchResults();
      const { executeToolbarSearch, searchText, isFiltered } = useArticleSearch();
      searchText.value = 'alpha';
      executeToolbarSearch();
      await vi.waitFor(() => expect(isFiltered.value).toBe(true));
    });

    it('isFiltered is true when tags filter is set', async () => {
      mockSearchResults();
      const { filter, applyFilters, isFiltered } = useArticleSearch();
      filter.tags = ['ml'];
      applyFilters();
      await vi.waitFor(() => expect(isFiltered.value).toBe(true));
    });

    it('activeTotalCount returns count for the active tab', async () => {
      mockSearchResults(sampleArticles, { ...sampleCounts, working: 25 });
      const { search, activeTotalCount } = useArticleSearch();
      await search();
      // Default tab is "working", statusCounts refreshed from fetchCounts
      expect(activeTotalCount.value).toBe(25);
    });

    it('selectedGlobalIndex computes 1-based global position', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'query_articles') return Promise.resolve(sampleArticles);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        if (cmd === 'get_article') {
          const id = args?.id as string;
          return Promise.resolve(sampleArticles.find((a) => a.id === id) ?? sampleArticles[0]);
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { search, selectArticle, selectedGlobalIndex } = useArticleSearch();
      await search();
      await selectArticle('a2'); // index 1 on page → global index 2
      expect(selectedGlobalIndex.value).toBe(2);
    });
  });

  // ── Route Params ───────────────────────────────────────────────────
  describe('applyRouteParams()', () => {
    it('sets active tab from status param', async () => {
      mockSearchResults();
      const { applyRouteParams, activeStatusTab } = useArticleSearch();
      await applyRouteParams({ status: 'rejected' });
      expect(activeStatusTab.value).toBe('rejected');
    });

    it('sets tag filters from tags param (resolved to names)', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      // The tags store mock has t1 → "machine-learning"
      await applyRouteParams({ tags: ['t1'] });
      expect(filter.tags).toEqual(['machine-learning']);
      expect(showFilters.value).toBe(true);
    });

    it('sets label filters from labels param (resolved to names)', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ labels: ['l1'] });
      expect(filter.labels).toEqual(['priority-read']);
      expect(showFilters.value).toBe(true);
    });

    it('ignores invalid status values', async () => {
      mockSearchResults();
      const { applyRouteParams, activeStatusTab } = useArticleSearch();
      await applyRouteParams({ status: 'invalid_status' });
      // Should remain at default
      expect(activeStatusTab.value).toBe('working');
    });

    // ── Characterization tests for branches extracted into helpers ──
    // These pin the behavior of each helper before the refactor so the
    // extraction is provably behavior-identical.

    it('maps status "error" to working + screeningErrorsOnly', async () => {
      mockSearchResults();
      const { applyRouteParams, activeStatusTab } = useArticleSearch();
      await applyRouteParams({ status: 'error' });
      expect(activeStatusTab.value).toBe('error');
      expect(tauriCommand).toHaveBeenCalledWith(
        'query_articles',
        expect.objectContaining({
          query: expect.objectContaining({ status: 'working', screeningErrorsOnly: true }),
        })
      );
    });

    it('maps status "all" to null query.status', async () => {
      mockSearchResults();
      const { applyRouteParams, activeStatusTab } = useArticleSearch();
      await applyRouteParams({ status: 'all' });
      expect(activeStatusTab.value).toBe('all');
      expect(tauriCommand).toHaveBeenCalledWith(
        'query_articles',
        expect.objectContaining({
          query: expect.objectContaining({ status: null, screeningErrorsOnly: false }),
        })
      );
    });

    it('clears screeningErrorsOnly when a non-error status is applied', async () => {
      mockSearchResults();
      const s = useArticleSearch();
      // First land on the error tab.
      await s.applyRouteParams({ status: 'error' });
      // Then navigate to a normal tab.
      await s.applyRouteParams({ status: 'included' });
      expect(tauriCommand).toHaveBeenCalledWith(
        'query_articles',
        expect.objectContaining({
          query: expect.objectContaining({ status: 'included', screeningErrorsOnly: false }),
        })
      );
    });

    it('syncs yearFrom and yearTo to both filter and query and opens the panel', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ yearFrom: 2018, yearTo: 2023 });
      expect(filter.yearFrom).toBe(2018);
      expect(filter.yearTo).toBe(2023);
      expect(showFilters.value).toBe(true);
      expect(tauriCommand).toHaveBeenCalledWith(
        'query_articles',
        expect.objectContaining({
          query: expect.objectContaining({ yearFrom: 2018, yearTo: 2023 }),
        })
      );
    });

    it('syncs journal to both filter and query and opens the panel', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ journal: 'Nature' });
      expect(filter.journal).toBe('Nature');
      expect(showFilters.value).toBe(true);
      expect(tauriCommand).toHaveBeenCalledWith(
        'query_articles',
        expect.objectContaining({
          query: expect.objectContaining({ journal: 'Nature' }),
        })
      );
    });

    it('syncs author to both filter.authorText and query.author and opens the panel', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ author: 'Alice' });
      expect(filter.authorText).toBe('Alice');
      expect(showFilters.value).toBe(true);
      expect(tauriCommand).toHaveBeenCalledWith(
        'query_articles',
        expect.objectContaining({
          query: expect.objectContaining({ author: 'Alice' }),
        })
      );
    });

    it('filters out unknown tag IDs during resolution', async () => {
      mockSearchResults();
      const { applyRouteParams, filter } = useArticleSearch();
      // t1 resolves; t999 does not exist in the mock store.
      await applyRouteParams({ tags: ['t1', 't999'] });
      expect(filter.tags).toEqual(['machine-learning']);
    });

    it('filters out unknown label IDs during resolution', async () => {
      mockSearchResults();
      const { applyRouteParams, filter } = useArticleSearch();
      await applyRouteParams({ labels: ['l1', 'l999'] });
      expect(filter.labels).toEqual(['priority-read']);
    });

    it('ignores empty tags and labels arrays (length-0 guard)', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ tags: [], labels: [] });
      expect(filter.tags).toEqual([]);
      expect(filter.labels).toEqual([]);
      // No tags/labels/year/journal/author → panel stays closed.
      expect(showFilters.value).toBe(false);
    });

    it('keeps showFilters false when filterCollapsed is true, even with tags', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ tags: ['t1'], filterCollapsed: true });
      expect(filter.tags).toEqual(['machine-learning']);
      expect(showFilters.value).toBe(false);
    });

    it('keeps showFilters false when filterCollapsed is true, even with labels', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ labels: ['l1'], filterCollapsed: true });
      expect(filter.labels).toEqual(['priority-read']);
      expect(showFilters.value).toBe(false);
    });

    it('keeps showFilters false when filterCollapsed is true, even with yearFrom', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ yearFrom: 2020, filterCollapsed: true });
      expect(filter.yearFrom).toBe(2020);
      expect(showFilters.value).toBe(false);
    });

    it('keeps showFilters false when filterCollapsed is true, even with journal', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ journal: 'Science', filterCollapsed: true });
      expect(filter.journal).toBe('Science');
      expect(showFilters.value).toBe(false);
    });

    it('keeps showFilters false when filterCollapsed is true, even with author', async () => {
      mockSearchResults();
      const { applyRouteParams, filter, showFilters } = useArticleSearch();
      await applyRouteParams({ author: 'Bob', filterCollapsed: true });
      expect(filter.authorText).toBe('Bob');
      expect(showFilters.value).toBe(false);
    });

    it('runs search() even when no params are provided', async () => {
      mockSearchResults();
      const { applyRouteParams } = useArticleSearch();
      await applyRouteParams({});
      expect(tauriCommand).toHaveBeenCalledWith('query_articles', expect.any(Object));
    });

    // ── resetFilters (decision D5) ──────────────────────────────────
    // The bibliometric deep-link envelope sets `resetFilters: '1'` so the
    // keep-alive-cached ArticleList clears any preserved filter/query state
    // before applying the fresh deep-link filter. Without this, a prior
    // session's filters would overlay the deep-link (e.g. landing on
    // `author="Bob" AND yearFrom=2020` instead of `author="Bob"` alone).
    describe('resetFilters (D5)', () => {
      it('clears stale yearFrom/tags/labels when resetFilters: true', async () => {
        mockSearchResults();
        const s = useArticleSearch();
        // Pre-populate stale filter state (simulating a prior session).
        s.filter.yearFrom = 2020;
        s.filter.tags = ['stale-tag'];
        s.filter.labels = ['stale-label'];
        s.applyFilters();
        // Now arrive via a biblio deep-link for author="Bob".
        await s.applyRouteParams({ author: 'Bob', resetFilters: true });
        // `search()` issues `query_articles` then `get_article_counts`, so we
        // assert with `toHaveBeenCalledWith` (any matching call) rather than
        // `toHaveBeenLastCalledWith` (the last call is always
        // `get_article_counts` after `await search()` resolves).
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({
              author: 'Bob',
              yearFrom: null,
              tags: [],
              labels: [],
            }),
          })
        );
      });

      it('preserves existing filters when resetFilters is absent (backward-compat)', async () => {
        mockSearchResults();
        const s = useArticleSearch();
        // Pre-populate filter state.
        s.filter.yearFrom = 2020;
        s.filter.tags = ['existing-tag'];
        s.applyFilters();
        // Arrive via a non-biblio deep-link (no resetFilters flag).
        await s.applyRouteParams({ author: 'Bob' });
        // Existing filters survive the overlay. Use `toHaveBeenCalledWith`
        // (not `Last`) because `search()` always issues `get_article_counts`
        // after `query_articles`.
        expect(tauriCommand).toHaveBeenCalledWith(
          'query_articles',
          expect.objectContaining({
            query: expect.objectContaining({
              author: 'Bob',
              yearFrom: 2020,
              tags: ['existing-tag'],
            }),
          })
        );
      });

      it('clears searchText when resetFilters: true', async () => {
        mockSearchResults();
        const s = useArticleSearch();
        // Simulate a stale toolbar search from a prior session.
        s.searchText.value = 'old query';
        s.executeToolbarSearch();
        await s.applyRouteParams({ author: 'Bob', resetFilters: true });
        expect(s.searchText.value).toBe('');
      });

      it('resets to page 1 when resetFilters: true', async () => {
        mockSearchResults();
        const s = useArticleSearch();
        // Simulate being on page 3 from a prior session.
        s.goToPage(3);
        await vi.waitFor(() => expect(s.currentPage.value).toBe(3));
        await s.applyRouteParams({ author: 'Bob', resetFilters: true });
        expect(s.currentPage.value).toBe(1);
      });
    });
  });

  // ── Article Mutations ──────────────────────────────────────────────
  describe('article mutations', () => {
    it('updateNotes calls Tauri command and refreshes article', async () => {
      const updated = { ...sampleArticles[0], userNotes: 'New note' };
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'update_article_notes') return Promise.resolve(undefined);
        if (cmd === 'get_article') return Promise.resolve(updated);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { updateNotes } = useArticleSearch();
      await updateNotes('a1', 'New note');
      expect(tauriCommand).toHaveBeenCalledWith('update_article_notes', {
        id: 'a1',
        notes: 'New note',
      });
    });

    it('updateTags calls Tauri command and refreshes tags store', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'update_article_tags') return Promise.resolve(undefined);
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { updateTags } = useArticleSearch();
      await updateTags('a1', ['t1']);
      expect(tauriCommand).toHaveBeenCalledWith('update_article_tags', {
        id: 'a1',
        tagIds: ['t1'],
      });
      expect(mockTagsStore.fetchTags).toHaveBeenCalled();
    });

    it('updateLabels calls Tauri command and refreshes labels store', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'update_article_labels') return Promise.resolve(undefined);
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { updateLabels } = useArticleSearch();
      await updateLabels('a1', ['l1']);
      expect(tauriCommand).toHaveBeenCalledWith('update_article_labels', {
        id: 'a1',
        labelIds: ['l1'],
      });
      expect(mockLabelsStore.fetchLabels).toHaveBeenCalled();
    });

    it('updateCriteria calls Tauri command', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'update_article_criteria') return Promise.resolve(undefined);
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { updateCriteria } = useArticleSearch();
      await updateCriteria('a1', ['inc1'], ['exc1']);
      expect(tauriCommand).toHaveBeenCalledWith('update_article_criteria', {
        id: 'a1',
        inclusionIds: ['inc1'],
        exclusionIds: ['exc1'],
      });
    });

    it('moveArticle updates status and auto-navigates', async () => {
      const fresh = { ...sampleArticles[0], status: 'included' as const };
      vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'update_article_status') return Promise.resolve(undefined);
        if (cmd === 'get_article') {
          // Return the fresh version for the moved article, or look up by id
          const id = args?.id as string;
          if (id === 'a1') return Promise.resolve(fresh);
          return Promise.resolve(sampleArticles.find((a) => a.id === id) ?? sampleArticles[1]);
        }
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        if (cmd === 'get_article_counts') return Promise.resolve(sampleCounts);
        if (cmd === 'query_articles') return Promise.resolve(sampleArticles);
        return Promise.resolve(undefined);
      });

      // Mock localStorage for auto-navigate
      const localStorageMock = { getItem: vi.fn(() => 'true'), setItem: vi.fn() };
      vi.stubGlobal('localStorage', localStorageMock);

      const { search, selectArticle, moveArticle } = useArticleSearch();
      await search();
      await selectArticle('a1');
      const result = await moveArticle('a1', 'included');
      expect(tauriCommand).toHaveBeenCalledWith('update_article_status', {
        id: 'a1',
        newStatus: 'included',
      });
      // Should have auto-navigated to next article
      expect(result.isLast).toBe(false);
      expect(result.didNavigate).toBe(true);
    });
  });

  // ── syncArticleToList ──────────────────────────────────────────────
  describe('syncArticleToList', () => {
    it('patches the article in the articles list', async () => {
      mockSearchResults();
      const { search, selectArticle } = useArticleSearch();
      await search();

      // Now select an article and manually verify the list patches
      const updated = { ...sampleArticles[0], title: 'Updated Title' };
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'get_article') return Promise.resolve(updated);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });
      await selectArticle('a1');
      // After select, the internal syncArticleToList is called from updateNotes etc.
      // Verify the article list was patched
      // Note: syncArticleToList is not directly returned, but tested indirectly
    });
  });

  // ── Back-stack Navigation / Reference Paper Return Targets ─────────
  describe('back-stack navigation / reference paper return targets', () => {
    it('stores returnToReferencePaperId when navigating to an article from reference paper detail', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { navigateToArticle, returnToReferencePaperId, hasReturnTarget } = useArticleSearch();

      expect(returnToReferencePaperId.value).toBeNull();
      expect(hasReturnTarget.value).toBe(false);

      await navigateToArticle('a1', 'ref-paper-123');

      expect(returnToReferencePaperId.value).toBe('ref-paper-123');
      expect(hasReturnTarget.value).toBe(true);
    });

    it('clears returnToReferencePaperId when closeDetail is called', async () => {
      vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
        if (cmd === 'get_article') return Promise.resolve(sampleArticles[0]);
        if (cmd === 'get_audit_trail') return Promise.resolve([]);
        return Promise.resolve(undefined);
      });

      const { navigateToArticle, returnToReferencePaperId, closeDetail, hasReturnTarget } =
        useArticleSearch();

      await navigateToArticle('a1', 'ref-paper-123');
      expect(returnToReferencePaperId.value).toBe('ref-paper-123');

      closeDetail();
      expect(returnToReferencePaperId.value).toBeNull();
      expect(hasReturnTarget.value).toBe(false);
    });
  });
});
