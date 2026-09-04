import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ref } from 'vue';
import { setActivePinia, createPinia } from 'pinia';
import type { OpenAlexResultItem, OpenAlexSearchResponse } from '@/types/openalex';

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

// `smartSearchAvailable` is now a computed over the canonical LLM-configured
// composable. Mock it with a controllable ref so tests can drive the gate.
const mockLlmConfigured = ref(false);
vi.mock('@/composables/use-llm-configured', () => ({
  useLlmConfigured: () => mockLlmConfigured,
}));

import { useOpenAlexStore } from '@/stores/openalex';

function makeResult(id: string, overrides: Partial<OpenAlexResultItem> = {}): OpenAlexResultItem {
  return {
    work: {
      id,
      doi: `10.1234/${id}`,
      title: `Article ${id}`,
      publicationYear: 2024,
      publicationDate: '2024-01-01',
      authorships: [],
      primaryLocation: null,
      abstractInvertedIndex: null,
      biblio: null,
      citedByCount: 0,
      language: 'en',
      keywords: [],
      type: 'article',
      openAccess: null,
      isRetracted: false,
      referencedWorks: [],
    },
    abstractText: 'Abstract text.',
    snippet: 'Abstract text.',
    alreadyInLibrary: false,
    ...overrides,
  };
}

function makeSearchResponse(
  results: OpenAlexResultItem[],
  overrides: Partial<OpenAlexSearchResponse> = {}
): OpenAlexSearchResponse {
  return {
    results,
    totalCount: results.length,
    page: 1,
    perPage: 25,
    ...overrides,
  };
}

describe('OpenAlex Store', () => {
  let tauriMock: ReturnType<
    typeof vi.mocked<(typeof import('@/composables/use-tauri-command'))['tauriCommand']>
  >;

  function shimLocalStorage(): Storage {
    const store = new Map<string, string>();
    return {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        store.set(k, v);
      },
      removeItem: (k: string) => {
        store.delete(k);
      },
      clear: () => store.clear(),
      key: (i: number) => Array.from(store.keys())[i] ?? null,
      get length() {
        return store.size;
      },
    } as Storage;
  }

  beforeEach(async () => {
    setActivePinia(createPinia());
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    tauriMock = vi.mocked(tauriCommand);
    tauriMock.mockReset();
    mockLlmConfigured.value = false;
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
  });

  it('search_updates_results', async () => {
    tauriMock.mockResolvedValueOnce(makeSearchResponse([makeResult('W1')]));

    const store = useOpenAlexStore();
    store.setQuery('test query');
    await store.search();

    expect(store.results).toHaveLength(1);
    expect(store.totalCount).toBe(1);
    expect(store.results[0]?.work.title).toBe('Article W1');
  });

  it('search_sets_error_on_empty_query', async () => {
    const store = useOpenAlexStore();
    await store.search();

    expect(store.error).toBe('Please enter a search query.');
    expect(tauriMock).not.toHaveBeenCalled();
  });

  it('search_sets_error_on_backend_failure', async () => {
    tauriMock.mockRejectedValueOnce(new Error('Network error'));

    const store = useOpenAlexStore();
    store.setQuery('test');
    await store.search();

    expect(store.error).toBe('Network error');
    expect(store.results).toEqual([]);
    expect(store.totalCount).toBe(0);
  });

  it('totalPages_is_capped_at_1000_per_page', () => {
    const store = useOpenAlexStore();
    store.$patch({ totalCount: 2500, perPage: 25 });
    expect(store.totalPages).toBe(40);
  });

  it('totalPages_rounds_up', () => {
    const store = useOpenAlexStore();
    store.$patch({ totalCount: 27, perPage: 25 });
    expect(store.totalPages).toBe(2);
  });

  it('selectableCount_excludes_already_in_library', () => {
    const store = useOpenAlexStore();
    store.$patch({
      results: [
        makeResult('W1', { alreadyInLibrary: false }),
        makeResult('W2', { alreadyInLibrary: true }),
        makeResult('W3', { alreadyInLibrary: false }),
      ],
    });
    expect(store.selectableCount).toBe(2);
  });

  it('cappedTotalCount_caps_at_1000', () => {
    const store = useOpenAlexStore();
    store.$patch({ totalCount: 2500 });
    expect(store.cappedTotalCount).toBe(1000);
  });

  it('selectedResult_finds_by_id', () => {
    const store = useOpenAlexStore();
    store.$patch({
      results: [makeResult('W1'), makeResult('W2')],
      selectedResultId: 'W2',
    });
    expect(store.selectedResult?.work.id).toBe('W2');
  });

  it('selectedResult_returns_null_when_not_found', () => {
    const store = useOpenAlexStore();
    store.$patch({
      results: [makeResult('W1')],
      selectedResultId: 'W99',
    });
    expect(store.selectedResult).toBeNull();
  });

  describe('selection', () => {
    it('toggleSelection_adds_and_removes', () => {
      const store = useOpenAlexStore();
      store.toggleSelection('W1');
      expect(store.selectedIds.has('W1')).toBe(true);
      store.toggleSelection('W1');
      expect(store.selectedIds.has('W1')).toBe(false);
    });

    it('selectAll_selects_only_not_in_library', () => {
      const store = useOpenAlexStore();
      store.$patch({
        results: [
          makeResult('W1', { alreadyInLibrary: false }),
          makeResult('W2', { alreadyInLibrary: true }),
          makeResult('W3', { alreadyInLibrary: false }),
        ],
      });
      store.selectAll();
      expect(store.selectedIds.has('W1')).toBe(true);
      expect(store.selectedIds.has('W2')).toBe(false);
      expect(store.selectedIds.has('W3')).toBe(true);
    });

    it('clearSelection_empties_selectedIds', () => {
      const store = useOpenAlexStore();
      store.toggleSelection('W1');
      store.clearSelection();
      expect(store.selectedIds.size).toBe(0);
    });
  });

  describe('navigation', () => {
    it('goToPage_ignores_page_below_1', () => {
      const store = useOpenAlexStore();
      store.$patch({ hasSearched: true });
      store.goToPage(0);
      expect(store.currentPage).toBe(1);
    });

    it('goToPage_ignores_page_above_totalPages', () => {
      const store = useOpenAlexStore();
      store.$patch({ totalCount: 20, perPage: 25, hasSearched: true });
      store.goToPage(2);
      expect(store.currentPage).toBe(1);
    });

    it('clearSearch_resets_all_state', () => {
      const store = useOpenAlexStore();
      store.$patch({
        query: 'old query',
        results: [makeResult('W1')],
        totalCount: 100,
        currentPage: 3,
        selectedResultId: 'W1',
        selectedIds: new Set(['W1']),
        error: 'some error',
        hasSearched: true,
      });

      store.clearSearch();

      expect(store.query).toBe('');
      expect(store.results).toEqual([]);
      expect(store.totalCount).toBe(0);
      expect(store.currentPage).toBe(1);
      expect(store.selectedResultId).toBeNull();
      expect(store.selectedIds.size).toBe(0);
      expect(store.error).toBeNull();
      expect(store.hasSearched).toBe(false);
    });
  });

  describe('setters_auto_search', () => {
    it('setFilters_resets_page_and_searches_when_has_searched', async () => {
      tauriMock.mockResolvedValue(makeSearchResponse([]));
      const store = useOpenAlexStore();
      store.$patch({ hasSearched: true, currentPage: 5, query: 'q' });

      store.setFilters({ ...store.filters, yearFrom: 2020 });
      expect(store.currentPage).toBe(1);
      expect(tauriMock).toHaveBeenCalled();
    });

    it('setFilters_does_not_search_when_has_not_searched', () => {
      const store = useOpenAlexStore();
      store.setFilters({ ...store.filters, yearFrom: 2020 });
      expect(tauriMock).not.toHaveBeenCalled();
    });

    it('setSort_resets_page_and_searches_when_has_searched', async () => {
      tauriMock.mockResolvedValue(makeSearchResponse([]));
      const store = useOpenAlexStore();
      store.$patch({ hasSearched: true, currentPage: 5, query: 'q' });

      store.setSort('publication_date:desc');
      expect(store.currentPage).toBe(1);
      expect(tauriMock).toHaveBeenCalled();
    });

    it('setPerPage_resets_page_and_searches_when_has_searched', async () => {
      tauriMock.mockResolvedValue(makeSearchResponse([]));
      const store = useOpenAlexStore();
      store.$patch({ hasSearched: true, currentPage: 5, query: 'q' });

      store.setPerPage(50);
      expect(store.currentPage).toBe(1);
      expect(store.perPage).toBe(50);
      expect(tauriMock).toHaveBeenCalled();
    });
  });

  describe('import', () => {
    it('importSelected_returns_null_when_nothing_selected', async () => {
      const store = useOpenAlexStore();
      store.$patch({ results: [makeResult('W1')] });
      const result = await store.importSelected();
      expect(result).toBeNull();
    });

    it('importSelected_skips_already_in_library', async () => {
      tauriMock.mockResolvedValueOnce({ importedCount: 1, skippedCount: 0 });
      tauriMock.mockResolvedValueOnce([]);
      const store = useOpenAlexStore();
      store.$patch({
        results: [
          makeResult('W1', { alreadyInLibrary: false }),
          makeResult('W2', { alreadyInLibrary: true }),
        ],
      });
      store.toggleSelection('W1');
      store.toggleSelection('W2');

      await store.importSelected();

      const importCall = tauriMock.mock.calls.find((c) => c[0] === 'import_openalex_articles');
      expect(importCall).toBeDefined();
      if (importCall) {
        const worksArg = importCall[1] as { works: Array<{ id: string }> };
        expect(worksArg.works).toHaveLength(1);
        expect(worksArg.works[0]!.id).toBe('W1');
      }
    });

    it('importSelected_sets_error_on_failure', async () => {
      tauriMock.mockRejectedValueOnce(new Error('Import failed'));
      const store = useOpenAlexStore();
      store.$patch({ results: [makeResult('W1')] });
      store.toggleSelection('W1');

      const result = await store.importSelected();

      expect(result).toBeNull();
      expect(store.error).toBe('Import failed');
    });

    it('importSingle_returns_null_when_not_in_results', async () => {
      const store = useOpenAlexStore();
      const result = await store.importSingle('W99');
      expect(result).toBeNull();
    });

    it('importSingle_returns_null_when_already_in_library', async () => {
      const store = useOpenAlexStore();
      store.$patch({ results: [makeResult('W1', { alreadyInLibrary: true })] });
      const result = await store.importSingle('W1');
      expect(result).toBeNull();
    });
  });

  describe('settings', () => {
    it('saveSettings_calls_backend_then_reloads', async () => {
      tauriMock.mockResolvedValueOnce(undefined);
      tauriMock.mockResolvedValueOnce({
        hasApiKey: true,
        mailto: 'test@example.com',
        retrieveReferences: true,
      });

      const store = useOpenAlexStore();
      await store.saveSettings({
        mailto: 'test@example.com',
        retrieveReferences: true,
      });

      expect(tauriMock).toHaveBeenCalledWith('set_openalex_settings', expect.any(Object));
      expect(tauriMock).toHaveBeenCalledWith('get_openalex_settings', {});
      expect(store.settings.mailto).toBe('test@example.com');
      expect(store.settings.retrieveReferences).toBe(true);
    });

    it('loadSettings_silently_ignores_error', async () => {
      tauriMock.mockRejectedValueOnce(new Error('fail'));
      const store = useOpenAlexStore();
      await store.loadSettings();
      expect(store.settings.mailto).toBe('');
      expect(store.settings.retrieveReferences).toBe(false);
    });
  });

  describe('smart_search', () => {
    it('smartSearch_populates_query_and_filters_then_searches', async () => {
      tauriMock
        .mockResolvedValueOnce({
          searchQuery: 'sugar tax obesity',
          suggestedFilters: {
            type: ['article'],
            publicationYear: '2020-2025',
          },
        } as import('@/types/openalex').SmartSearchQuery)
        .mockResolvedValueOnce(makeSearchResponse([makeResult('W1')]));

      const store = useOpenAlexStore();
      store.setQuery('');
      await store.smartSearch();

      expect(store.query).toBe('sugar tax obesity');
      expect(store.filters.yearFrom).toBe(2020);
      expect(store.filters.yearTo).toBe(2025);
      expect(store.filters.workTypes).toContain('article');
      expect(store.hasSearched).toBe(true);
      expect(store.results).toHaveLength(1);
    });

    it('smartSearch_sets_error_on_failure', async () => {
      tauriMock.mockRejectedValueOnce(new Error('LLM error'));
      const store = useOpenAlexStore();
      await store.smartSearch();
      expect(store.error).toBe('LLM error');
      expect(store.smartSearchLoading).toBe(false);
    });

    it('smartSearchAvailable_tracks_the_llm_configured_gate', () => {
      // `smartSearchAvailable` is a computed over `useLlmConfigured()`, so it
      // reactively follows the mock ref. No IPC probe is involved anymore.
      mockLlmConfigured.value = false;
      const store = useOpenAlexStore();
      expect(store.smartSearchAvailable).toBe(false);

      mockLlmConfigured.value = true;
      expect(store.smartSearchAvailable).toBe(true);

      mockLlmConfigured.value = false;
      expect(store.smartSearchAvailable).toBe(false);
    });
  });

  it('pagination_state', async () => {
    tauriMock.mockResolvedValue(makeSearchResponse([], { totalCount: 100, page: 2, perPage: 50 }));

    const store = useOpenAlexStore();
    store.setQuery('test');
    await store.search();

    store.setPerPage(50);
    expect(store.currentPage).toBe(1);
    expect(store.perPage).toBe(50);

    store.goToPage(2);
    expect(store.currentPage).toBe(2);
  });

  it('library_doi_check_greys_out', async () => {
    tauriMock
      .mockResolvedValueOnce(
        makeSearchResponse(
          [
            makeResult('W1', {
              work: { ...makeResult('W1').work, doi: 'https://doi.org/10.1234/in_library' },
            }),
            makeResult('W2', {
              work: { ...makeResult('W2').work, doi: 'https://doi.org/10.5678/not_in_library' },
            }),
          ],
          { totalCount: 2 }
        )
      )
      .mockResolvedValueOnce(['10.1234/in_library']);

    const store = useOpenAlexStore();
    store.setQuery('test');
    await store.search();
    await store.refreshLibraryFlags();

    expect(store.results[0]?.alreadyInLibrary).toBe(true);
    expect(store.results[1]?.alreadyInLibrary).toBe(false);
  });

  it('library_doi_check_greys_out_case_variant', async () => {
    tauriMock
      .mockResolvedValueOnce(
        makeSearchResponse(
          [
            makeResult('W1', {
              work: { ...makeResult('W1').work, doi: 'https://doi.org/10.1234/In_Library' },
            }),
          ],
          { totalCount: 1 }
        )
      )
      // Backend returns a stored-casing value; the store must normalize
      // (trim + lowercase) before the Set comparison so the grey-out fires.
      .mockResolvedValueOnce(['10.1234/In_Library']);

    const store = useOpenAlexStore();
    store.setQuery('test');
    await store.search();
    await store.refreshLibraryFlags();

    expect(store.results[0]?.alreadyInLibrary).toBe(true);
  });

  it('smart_search_mode_gated_on_llm_configured', () => {
    // `smartSearchAvailable` is now a computed over `useLlmConfigured()`; no
    // `has_llm_config` IPC probe is involved. Mutate the mock ref and the
    // store's computed follows.
    mockLlmConfigured.value = false;
    const store = useOpenAlexStore();
    expect(store.smartSearchAvailable).toBe(false);

    mockLlmConfigured.value = true;
    expect(store.smartSearchAvailable).toBe(true);
  });

  it('reference_harvest_toggle_defaults_off', async () => {
    tauriMock.mockResolvedValueOnce({
      hasApiKey: false,
      mailto: '',
      retrieveReferences: false,
    });

    const store = useOpenAlexStore();
    await store.loadSettings();

    expect(store.settings.retrieveReferences).toBe(false);
  });
});
