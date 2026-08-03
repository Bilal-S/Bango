import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

import { tauriCommand } from '@/composables/use-tauri-command';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import type {
  OpenAlexResultItem,
  OpenAlexSearchResponse,
  OpenAlexFilters,
  OpenAlexSettings,
  OpenAlexSettingsInput,
  SmartSearchQuery,
} from '@/types/openalex';
import { DEFAULT_OPENALEX_FILTERS } from '@/types/openalex';

export const useOpenAlexStore = defineStore('openalex', () => {
  const query = ref('');
  const results = ref<OpenAlexResultItem[]>([]);
  const totalCount = ref(0);
  const currentPage = ref(1);
  const perPage = ref(25);
  const sortBy = ref('relevance_score:desc');
  const filters = ref<OpenAlexFilters>({ ...DEFAULT_OPENALEX_FILTERS });
  const selectedResultId = ref<string | null>(null);
  const selectedIds = ref<Set<string>>(new Set());
  const loading = ref(false);
  const error = ref<string | null>(null);
  const smartSearchLoading = ref(false);
  /**
   * Reactive "Smart Search is available" gate, derived from the canonical
   * LLM-configured composable. Smart Search requires an LLM to generate the
   * OpenAlex Boolean query from aims + criteria, so this mirrors the same
   * gate every other LLM-dependent feature uses. Replaces the former one-shot
   * `has_llm_config` IPC probe (`checkSmartSearchAvailability`) which went
   * stale on Settings edits.
   */
  const smartSearchAvailable = useLlmConfigured();
  const hasSearched = ref(false);

  // Settings
  const settings = ref<OpenAlexSettings>({
    hasApiKey: false,
    mailto: '',
    retrieveReferences: false,
  });

  const totalPages = computed(() =>
    Math.min(Math.ceil(totalCount.value / perPage.value), Math.ceil(1000 / perPage.value))
  );

  const selectedResult = computed(
    () => results.value.find((r) => r.work.id === selectedResultId.value) ?? null
  );

  const selectedCount = computed(() => selectedIds.value.size);

  const cappedTotalCount = computed(() => Math.min(totalCount.value, 1000));

  /** Count of results that are NOT already in the library (selectable). */
  const selectableCount = computed(() => results.value.filter((r) => !r.alreadyInLibrary).length);

  async function search(): Promise<void> {
    if (!query.value.trim()) {
      error.value = 'Please enter a search query.';
      return;
    }
    loading.value = true;
    error.value = null;
    try {
      const response = await tauriCommand<OpenAlexSearchResponse>('search_openalex', {
        params: {
          query: query.value,
          filters: filters.value,
          sort: sortBy.value,
          perPage: perPage.value,
          page: currentPage.value,
        },
      });
      results.value = response.results;
      totalCount.value = response.totalCount;
      hasSearched.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      results.value = [];
      totalCount.value = 0;
    } finally {
      loading.value = false;
    }
  }

  function setQuery(q: string): void {
    query.value = q;
  }

  function setFilters(newFilters: OpenAlexFilters): void {
    filters.value = { ...newFilters };
    currentPage.value = 1;
    if (hasSearched.value) {
      void search();
    }
  }

  function setSort(sort: string): void {
    sortBy.value = sort;
    currentPage.value = 1;
    if (hasSearched.value) {
      void search();
    }
  }

  function setPerPage(size: number): void {
    perPage.value = size;
    currentPage.value = 1;
    if (hasSearched.value) {
      void search();
    }
  }

  function goToPage(page: number): void {
    if (page < 1 || page > totalPages.value) return;
    currentPage.value = page;
    void search();
  }

  function selectResult(workId: string | null): void {
    selectedResultId.value = workId;
  }

  function toggleSelection(workId: string): void {
    const next = new Set(selectedIds.value);
    if (next.has(workId)) {
      next.delete(workId);
    } else {
      next.add(workId);
    }
    selectedIds.value = next;
  }

  function selectAll(): void {
    // Only select results that are not already in the library.
    selectedIds.value = new Set(
      results.value.filter((r) => !r.alreadyInLibrary).map((r) => r.work.id)
    );
  }

  function clearSelection(): void {
    selectedIds.value = new Set();
  }

  function clearSearch(): void {
    query.value = '';
    results.value = [];
    totalCount.value = 0;
    currentPage.value = 1;
    selectedResultId.value = null;
    selectedIds.value = new Set();
    error.value = null;
    hasSearched.value = false;
  }

  /** Read the auto-summarize + section-summaries localStorage flags so the
   * backend import pipeline can run the AI summary after a successful PDF
   * attach (mirrors the manual attach path's `onAttached` hook). */
  function readAutoSummarizeFlags(): {
    autoSummarize: boolean;
    includeSectionSummaries: boolean;
  } {
    return {
      autoSummarize: localStorage.getItem('bango-full-text-summaries') === 'true',
      includeSectionSummaries: localStorage.getItem('bango-section-summaries') === 'true',
    };
  }

  async function importSelected(): Promise<{ importedCount: number; skippedCount: number } | null> {
    const worksToImport = results.value
      .filter((r) => selectedIds.value.has(r.work.id) && !r.alreadyInLibrary)
      .map((r) => r.work);

    if (worksToImport.length === 0) {
      return null;
    }

    try {
      const flags = readAutoSummarizeFlags();
      const result = await tauriCommand<{ importedCount: number; skippedCount: number }>(
        'import_openalex_articles',
        {
          works: worksToImport,
          autoSummarize: flags.autoSummarize,
          includeSectionSummaries: flags.includeSectionSummaries,
        }
      );
      clearSelection();
      // Refresh the library DOI check for the current results.
      await refreshLibraryFlags();
      return result;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    }
  }

  /** Import a single work by its OpenAlex ID. Used by the detail panel "Add" button. */
  async function importSingle(
    workId: string
  ): Promise<{ importedCount: number; skippedCount: number } | null> {
    const item = results.value.find((r) => r.work.id === workId);
    if (!item || item.alreadyInLibrary) return null;

    try {
      const flags = readAutoSummarizeFlags();
      const result = await tauriCommand<{ importedCount: number; skippedCount: number }>(
        'import_openalex_articles',
        {
          works: [item.work],
          autoSummarize: flags.autoSummarize,
          includeSectionSummaries: flags.includeSectionSummaries,
        }
      );
      // Update the alreadyInLibrary flag for this result.
      await refreshLibraryFlags();
      return result;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    }
  }

  async function refreshLibraryFlags(): Promise<void> {
    if (results.value.length === 0) return;
    const dois = results.value
      .map((r) => r.work.doi)
      .filter((d): d is string => d !== null)
      .map((d) => d.replace(/^https?:\/\/doi\.org\//i, '').toLowerCase());

    if (dois.length === 0) return;

    try {
      const libraryDois = await tauriCommand<string[]>('check_dois_in_library', { dois });
      const librarySet = new Set(libraryDois);
      results.value = results.value.map((r) => {
        const normalizedDoi = r.work.doi?.replace(/^https?:\/\/doi\.org\//i, '').toLowerCase();
        return {
          ...r,
          alreadyInLibrary: normalizedDoi ? librarySet.has(normalizedDoi) : false,
        };
      });
    } catch {
      // Non-fatal: just leave the flags as they are.
    }
  }

  async function loadSettings(): Promise<void> {
    try {
      settings.value = await tauriCommand<OpenAlexSettings>('get_openalex_settings', {});
    } catch {
      // Non-fatal: use defaults.
    }
  }

  async function saveSettings(input: OpenAlexSettingsInput): Promise<void> {
    await tauriCommand('set_openalex_settings', { settings: input });
    await loadSettings();
  }

  async function smartSearch(): Promise<void> {
    smartSearchLoading.value = true;
    error.value = null;
    try {
      const result = await tauriCommand<SmartSearchQuery>('smart_search_openalex', {});
      // Land the generated query in the editable search box.
      query.value = result.searchQuery;
      // Auto-populate filters from suggested filters.
      if (result.suggestedFilters.publicationYear) {
        const [fromStr, toStr] = result.suggestedFilters.publicationYear.split('-');
        filters.value = {
          ...filters.value,
          yearFrom: fromStr ? Number(fromStr) : null,
          yearTo: toStr ? Number(toStr) : null,
          workTypes:
            result.suggestedFilters.type.length > 0
              ? [...result.suggestedFilters.type]
              : filters.value.workTypes,
        };
      }
      currentPage.value = 1;
      // Auto-execute the search so the user sees results immediately.
      await search();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      smartSearchLoading.value = false;
    }
  }

  return {
    // State
    query,
    results,
    totalCount,
    currentPage,
    perPage,
    sortBy,
    filters,
    selectedResultId,
    selectedIds,
    loading,
    error,
    smartSearchLoading,
    smartSearchAvailable,
    hasSearched,
    settings,
    // Computed
    totalPages,
    selectedResult,
    selectedCount,
    selectableCount,
    cappedTotalCount,
    // Actions
    search,
    setQuery,
    setFilters,
    setSort,
    setPerPage,
    goToPage,
    selectResult,
    toggleSelection,
    selectAll,
    clearSelection,
    clearSearch,
    importSelected,
    importSingle,
    refreshLibraryFlags,
    loadSettings,
    saveSettings,
    smartSearch,
  };
});
