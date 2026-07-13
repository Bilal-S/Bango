import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

import { useOpenAlexStore } from '@/stores/openalex';

describe('OpenAlex Store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('search_updates_results', async () => {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const mocked = vi.mocked(tauriCommand);
    mocked.mockResolvedValueOnce({
      results: [
        {
          work: {
            id: 'W1',
            doi: '10.1234/test',
            title: 'Test Article',
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
          abstractText: 'Test abstract',
          snippet: 'Test abstract',
          alreadyInLibrary: false,
        },
      ],
      totalCount: 1,
      page: 1,
      perPage: 25,
    });

    const store = useOpenAlexStore();
    store.setQuery('test query');
    await store.search();

    expect(store.results).toHaveLength(1);
    expect(store.totalCount).toBe(1);
    expect(store.results[0]?.work.title).toBe('Test Article');
  });

  it('pagination_state', async () => {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const mocked = vi.mocked(tauriCommand);
    mocked.mockResolvedValue({
      results: [],
      totalCount: 100,
      page: 2,
      perPage: 50,
    });

    const store = useOpenAlexStore();
    store.setQuery('test');
    await store.search();

    // Changing perPage should reset to page 1 and trigger a re-search
    store.setPerPage(50);
    expect(store.currentPage).toBe(1);
    expect(store.perPage).toBe(50);

    // Changing page should trigger a search
    store.goToPage(2);
    expect(store.currentPage).toBe(2);
  });

  it('library_doi_check_greys_out', async () => {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const mocked = vi.mocked(tauriCommand);

    // First call: search returns results with DOIs
    mocked.mockResolvedValueOnce({
      results: [
        {
          work: {
            id: 'W1',
            doi: 'https://doi.org/10.1234/in_library',
            title: 'In Library',
            publicationYear: 2024,
            publicationDate: null,
            authorships: [],
            primaryLocation: null,
            abstractInvertedIndex: null,
            biblio: null,
            citedByCount: 0,
            language: null,
            keywords: [],
            type: null,
            openAccess: null,
            isRetracted: false,
            referencedWorks: [],
          },
          abstractText: '',
          snippet: '',
          alreadyInLibrary: false,
        },
        {
          work: {
            id: 'W2',
            doi: 'https://doi.org/10.5678/not_in_library',
            title: 'Not In Library',
            publicationYear: 2024,
            publicationDate: null,
            authorships: [],
            primaryLocation: null,
            abstractInvertedIndex: null,
            biblio: null,
            citedByCount: 0,
            language: null,
            keywords: [],
            type: null,
            openAccess: null,
            isRetracted: false,
            referencedWorks: [],
          },
          abstractText: '',
          snippet: '',
          alreadyInLibrary: false,
        },
      ],
      totalCount: 2,
      page: 1,
      perPage: 25,
    });

    // Second call: check_dois_in_library returns the first DOI
    mocked.mockResolvedValueOnce(['10.1234/in_library']);

    const store = useOpenAlexStore();
    store.setQuery('test');
    await store.search();
    await store.refreshLibraryFlags();

    expect(store.results[0]?.alreadyInLibrary).toBe(true);
    expect(store.results[1]?.alreadyInLibrary).toBe(false);
  });

  it('smart_search_mode_gated_on_llm_configured', async () => {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const mocked = vi.mocked(tauriCommand);

    // LLM not configured
    mocked.mockResolvedValueOnce(false);
    const store = useOpenAlexStore();
    await store.checkSmartSearchAvailability();
    expect(store.smartSearchAvailable).toBe(false);

    // LLM configured
    mocked.mockResolvedValueOnce(true);
    await store.checkSmartSearchAvailability();
    expect(store.smartSearchAvailable).toBe(true);
  });

  it('reference_harvest_toggle_defaults_off', async () => {
    const { tauriCommand } = await import('@/composables/use-tauri-command');
    const mocked = vi.mocked(tauriCommand);
    mocked.mockResolvedValueOnce({
      hasApiKey: false,
      mailto: '',
      retrieveReferences: false,
    });

    const store = useOpenAlexStore();
    await store.loadSettings();

    expect(store.settings.retrieveReferences).toBe(false);
  });
});
