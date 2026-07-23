import { ref, reactive, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
import { useArticlePagination } from './use-article-pagination';
import { useArticleSelection } from './use-article-selection';
import { useArticleFullText } from './use-article-full-text';
import { useArticlesStore } from '@/stores/articles';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import type { Article, AuditEntry, ArticleStatus, ArticleCounts } from '@/types';

export type TitleMatchType = 'starts_with' | 'contains' | 'ends_with' | 'exact';

export interface ArticleFilter {
  titleMatch: TitleMatchType;
  titleText: string;
  authorText: string;
  yearFrom: number | null;
  yearTo: number | null;
  journal: string;
  /**
   * Case-insensitive partial-match DOI text. Empty string filters nothing.
   * Mutually exclusive with `doiEmpty`: when `doiEmpty` is true the backend
   * ignores this text and matches only empty/NULL DOIs.
   */
  doiText: string;
  /** When true, restrict to articles with no DOI (`doi IS NULL OR doi = ''`). */
  doiEmpty: boolean;
  tags: string[];
  labels: string[];
  /**
   * Tags the article must NOT have (NOT-filter). The Article list filter
   * panel toggles a tag pill between inclusion (`tags`) and exclusion
   * (`excludedTags`); the backend emits a `NOT IN` clause for these.
   */
  excludedTags: string[];
  /** Labels the article must NOT have (NOT-filter). See {@link excludedTags}. */
  excludedLabels: string[];
}

interface ArticleQuery {
  status: string | null;
  search: string | null;
  sortBy: string | null;
  sortDir: string | null;
  yearFrom: number | null;
  yearTo: number | null;
  manualOverrideOnly: boolean;
  screeningErrorsOnly: boolean;
  author: string | null;
  journal: string | null;
  /**
   * Partial-match DOI text sent to the backend (`doi` field). Null/empty
   * filters nothing. Mutually exclusive with `doiEmpty` at the UI layer.
   */
  doi: string | null;
  /** When true, restrict to articles with no DOI. */
  doiEmpty: boolean;
  tags: string[];
  labels: string[];
  excludedTags: string[];
  excludedLabels: string[];
  limit: number;
  offset: number;
}

type SortDirection = 'asc' | 'desc';

const STATUS_TABS: readonly (ArticleStatus | 'all' | 'error' | 'references' | 'search')[] = [
  'all',
  'duplicate',
  'working',
  'included',
  'rejected',
  'error',
  'references',
  'search',
] as const;

type StatusTab = (typeof STATUS_TABS)[number];

export function useArticleSearch() {
  const articlesStore = useArticlesStore();
  const tagsStore = useTagsStore();
  const labelsStore = useLabelsStore();

  const articles = ref<Article[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const selectedArticle = ref<Article | null>(null);
  const auditTrail = ref<AuditEntry[]>([]);
  const showDetail = ref(false);
  const returnToArticleId = ref<string | null>(null);
  const returnToReferencePaperId = ref<string | null>(null);

  // Multi-select (extracted composable)
  const {
    selectedIds,
    selectedCount,
    allSelected,
    someSelected,
    toggleSelect,
    toggleSelectRange,
    toggleSelectAll,
    clearSelection,
  } = useArticleSelection({ articles });

  // Smart default tab: Working > Included > All
  const defaultTab: StatusTab =
    articlesStore.byStatus.working > 0
      ? 'working'
      : articlesStore.byStatus.included > 0
        ? 'included'
        : 'all';
  const activeStatusTab = ref<StatusTab>(defaultTab);
  const showFilters = ref(false);

  const sortColumn = ref<string | null>(null);
  const sortDirection = ref<SortDirection>('asc');

  const filter = reactive<ArticleFilter>({
    titleMatch: 'contains',
    titleText: '',
    authorText: '',
    yearFrom: null,
    yearTo: null,
    journal: '',
    doiText: '',
    doiEmpty: false,
    tags: [],
    labels: [],
    excludedTags: [],
    excludedLabels: [],
  });

  const searchText = ref('');

  const query = reactive<ArticleQuery>({
    status: defaultTab === 'all' ? null : defaultTab,
    search: null,
    sortBy: null,
    sortDir: null,
    yearFrom: null,
    yearTo: null,
    manualOverrideOnly: false,
    screeningErrorsOnly: false,
    author: null,
    journal: null,
    doi: null,
    doiEmpty: false,
    tags: [],
    labels: [],
    excludedTags: [],
    excludedLabels: [],
    limit: 10,
    offset: 0,
  });

  const statusCounts = ref<ArticleCounts & { search?: number }>({
    // Seed from the pre-warmed store so counts render immediately
    // without waiting for the get_article_counts IPC round-trip.
    all: articlesStore.totalImported,
    duplicate: articlesStore.byStatus.duplicate,
    working: articlesStore.byStatus.working,
    included: articlesStore.byStatus.included,
    rejected: articlesStore.byStatus.rejected,
    error: 0,
    references: 0,
    search: 0,
  });

  async function fetchCounts(): Promise<void> {
    try {
      statusCounts.value = await tauriCommand<ArticleCounts>('get_article_counts', {});
    } catch (e: unknown) {
      console.error('Failed to fetch article counts', e);
    }
  }

  const allAuthors = computed((): string[] => {
    const authorSet = new Set<string>();
    for (const article of articles.value) {
      for (const author of article.authors) {
        authorSet.add(author);
      }
    }
    return Array.from(authorSet).sort();
  });

  const allTags = computed((): string[] => {
    return tagsStore.tags.map((t) => t.name).sort();
  });

  const allLabels = computed((): string[] => {
    return labelsStore.labels.map((l) => l.name).sort();
  });

  async function search(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      articles.value = await tauriCommand<Article[]>('query_articles', { query });
      await fetchCounts();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  // ── Pagination (extracted composable) ───────────────────────────────
  const isFiltered = computed(() => {
    return !!(
      query.search ||
      query.author ||
      query.journal ||
      query.doi ||
      query.doiEmpty ||
      query.tags.length > 0 ||
      query.labels.length > 0 ||
      query.excludedTags.length > 0 ||
      query.excludedLabels.length > 0 ||
      query.yearFrom !== null ||
      query.yearTo !== null
    );
  });

  const activeTotalCount = computed(() => {
    const tab = activeStatusTab.value;
    if (tab === 'all') return statusCounts.value.all;
    if (tab === 'error') return statusCounts.value.error;
    return statusCounts.value[tab as ArticleStatus] ?? 0;
  });

  const {
    pageSize,
    currentPage,
    totalPages,
    canGoPrev,
    canGoNext,
    selectedIndex,
    selectedGlobalIndex,
    resultCount,
    rangeStart,
    rangeEnd,
    resetPage,
    goToPage,
    changePageSize,
  } = useArticlePagination({
    articles,
    selectedArticle,
    query,
    statusCounts,
    activeStatusTab,
    search,
    isFiltered,
    activeTotalCount,
  });

  function setStatusTab(tab: StatusTab): void {
    activeStatusTab.value = tab;
    // "references" + "search" tabs: no article query needed - the components
    // handle their own data (ReferencesView / OpenAlexSearch).
    if (tab === 'references' || tab === 'search') {
      return;
    }
    // "error" tab: show working articles that have screening errors
    if (tab === 'error') {
      query.status = 'working';
      query.screeningErrorsOnly = true;
    } else {
      query.status = tab === 'all' ? null : tab;
      query.screeningErrorsOnly = false;
    }
    resetPage();
    void search();
  }

  function toggleSort(column: string): void {
    if (sortColumn.value === column) {
      sortDirection.value = sortDirection.value === 'asc' ? 'desc' : 'asc';
    } else {
      sortColumn.value = column;
      sortDirection.value = 'asc';
    }
    query.sortBy = sortColumn.value;
    query.sortDir = sortDirection.value;
    // Keep the current page - re-sort in-place so the user sees the
    // same offset with the new sort order applied.
    query.offset = (currentPage.value - 1) * pageSize.value;
    void search();
  }

  function toggleFilters(): void {
    showFilters.value = !showFilters.value;
  }

  function applyFilters(): void {
    query.search = filter.titleText || null;
    query.yearFrom = filter.yearFrom;
    query.yearTo = filter.yearTo;
    query.author = filter.authorText || null;
    query.journal = filter.journal || null;
    // DOI filter: when `doiEmpty` is checked the text is ignored (the backend
    // `doi_empty` branch wins over `doi` to avoid contradictory SQL).
    query.doiEmpty = filter.doiEmpty;
    query.doi = filter.doiEmpty ? null : filter.doiText.trim() || null;
    query.tags = [...filter.tags];
    query.labels = [...filter.labels];
    query.excludedTags = [...filter.excludedTags];
    query.excludedLabels = [...filter.excludedLabels];
    resetPage();
    void search();
  }

  function clearFilters(): void {
    filter.titleMatch = 'contains';
    filter.titleText = '';
    filter.authorText = '';
    filter.yearFrom = null;
    filter.yearTo = null;
    filter.journal = '';
    filter.doiText = '';
    filter.doiEmpty = false;
    filter.tags = [];
    filter.labels = [];
    filter.excludedTags = [];
    filter.excludedLabels = [];
    query.search = null;
    query.yearFrom = null;
    query.yearTo = null;
    query.author = null;
    query.journal = null;
    query.doi = null;
    query.doiEmpty = false;
    query.tags = [];
    query.labels = [];
    query.excludedTags = [];
    query.excludedLabels = [];
    resetPage();
    void search();
  }

  async function selectArticle(id: string): Promise<void> {
    try {
      selectedArticle.value = await tauriCommand<Article>('get_article', { id });
      auditTrail.value = await tauriCommand<AuditEntry[]>('get_audit_trail', { articleId: id });
      showDetail.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  /**
   * Refresh the article detail + table row after an operation that changes the
   * article without going through the explicit status-move path (e.g. AI
   * screening completion, AI summary generation, translation). Mirrors what
   * `moveArticle` does for the table: re-fetch the article (updates
   * `selectedArticle`), patch the articles list row so the table redraws (e.g.
   * the status color bar updates), and refresh status tab counts in the
   * background. Falls back to `selectArticle` if the article is no longer in
   * the list (e.g. status changed + filter excludes it).
   */
  async function refreshArticle(id: string): Promise<void> {
    await selectArticle(id);
    syncArticleToList(id);
    void fetchCounts();
  }

  async function moveArticle(
    id: string,
    newStatus: string
  ): Promise<{ isLast: boolean; didNavigate: boolean }> {
    await tauriCommand('update_article_status', { id, newStatus });
    // Re-fetch the article so we get the updated changedAt from the backend
    const fresh = await tauriCommand<Article>('get_article', { id });
    // Patch the article in-place to reflect new status + changedAt without a full redraw
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1, fresh);
    }
    const isLast = !hasNext.value;
    const autoNavigate = localStorage.getItem('bango-auto-navigate-after-decision') !== 'false';
    let didNavigate = false;
    if (!isLast && autoNavigate) {
      await navigateNext();
      didNavigate = true;
    } else {
      selectedArticle.value = fresh;
      auditTrail.value = await tauriCommand<AuditEntry[]>('get_audit_trail', { articleId: id });
    }
    // Refresh counts in the background (e.g. tab badges)
    void fetchCounts();
    return { isLast, didNavigate };
  }

  /** Patch the articles list row with the latest selectedArticle data so the table redraws. */
  function syncArticleToList(id: string): void {
    if (!selectedArticle.value || selectedArticle.value.id !== id) return;
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1, { ...selectedArticle.value });
    }
  }

  async function updateNotes(id: string, notes: string): Promise<void> {
    await tauriCommand('update_article_notes', { id, notes });
    await selectArticle(id);
    syncArticleToList(id);
  }

  /**
   * Update a single metadata field (Authors, Affiliation, Journal, Year, Lang,
   * DOI, Keywords) on an article via the in-place editor in the Metadata card.
   * Mirrors `updateNotes`: invoke -> re-fetch -> patch the table row.
   *
   * `field` is the snake_case DB column name (e.g. `"publication_year"`); the
   * backend `ArticleMetaField` enum (`#[serde(rename_all = "camelCase")]`)
   * expects `"publicationYear"` so we forward the camelCase key the editor
   * emits. The `value` is a scalar string for the text fields, or a `string[]`
   * for the array fields (`authors`, `keywords`).
   */
  async function updateMetadata(
    id: string,
    field: string,
    value: string | string[]
  ): Promise<void> {
    await tauriCommand('update_article_metadata', { id, field, value });
    await selectArticle(id);
    syncArticleToList(id);
  }

  async function updateTags(id: string, tagIds: string[]): Promise<void> {
    await tauriCommand('update_article_tags', { id, tagIds });
    await selectArticle(id);
    syncArticleToList(id);
    await tagsStore.fetchTags();
  }

  async function updateLabels(id: string, labelIds: string[]): Promise<void> {
    await tauriCommand('update_article_labels', { id, labelIds });
    await selectArticle(id);
    syncArticleToList(id);
    await labelsStore.fetchLabels();
  }

  async function updateCriteria(
    id: string,
    inclusionIds: string[],
    exclusionIds: string[]
  ): Promise<void> {
    await tauriCommand('update_article_criteria', { id, inclusionIds, exclusionIds });
    await selectArticle(id);
    syncArticleToList(id);
  }

  // ── Full text (extracted composable) ─────────────────────────────
  const { attachFullText, deleteFullTextAttachment, readFullTextContent, getFullTextFilePath } =
    useArticleFullText({ selectArticle, syncArticleToList, fetchCounts });

  const hasPrevious = computed(() => {
    if (selectedIndex.value > 0) return true;
    // Can go to previous page (and that page has articles)
    return currentPage.value > 1;
  });

  const hasNext = computed(() => {
    const idx = selectedIndex.value;
    if (idx >= 0 && idx < articles.value.length - 1) return true;
    // Can go to next page (and that page has articles)
    return currentPage.value < totalPages.value;
  });

  async function navigatePrev(): Promise<void> {
    if (!hasPrevious.value) return;
    if (selectedIndex.value > 0) {
      // Previous article on current page
      const prev = articles.value[selectedIndex.value - 1];
      if (prev) await selectArticle(prev.id);
    } else if (currentPage.value > 1) {
      // Cross to previous page - load it and select the last article
      const prevPage = currentPage.value - 1;
      currentPage.value = prevPage;
      query.offset = (prevPage - 1) * pageSize.value;
      await search();
      const lastOnPrevPage = articles.value[articles.value.length - 1];
      if (lastOnPrevPage) await selectArticle(lastOnPrevPage.id);
    }
  }

  async function navigateNext(): Promise<void> {
    if (!hasNext.value) return;
    if (selectedIndex.value < articles.value.length - 1) {
      // Next article on current page
      const next = articles.value[selectedIndex.value + 1];
      if (next) await selectArticle(next.id);
    } else if (currentPage.value < totalPages.value) {
      // Cross to next page - load it and select the first article
      const nextPage = currentPage.value + 1;
      currentPage.value = nextPage;
      query.offset = (nextPage - 1) * pageSize.value;
      await search();
      const firstOnNextPage = articles.value[0];
      if (firstOnNextPage) await selectArticle(firstOnNextPage.id);
    }
  }

  /** Execute a quick search from the toolbar search box. */
  function executeToolbarSearch(): void {
    query.search = searchText.value || null;
    resetPage();
    void search();
  }

  /** Clear the toolbar search and refresh results. */
  function clearSearch(): void {
    searchText.value = '';
    query.search = null;
    resetPage();
    void search();
  }

  // ── Bulk operations ───────────────────────────────────────────────
  async function bulkUpdateStatus(ids: string[], newStatus: string): Promise<void> {
    await tauriCommand('bulk_update_article_status', { ids, newStatus });
    clearSelection();
    await search();
  }

  async function bulkAddTag(ids: string[], tagName: string): Promise<void> {
    await tauriCommand('bulk_add_tag_to_articles', { articleIds: ids, tagName });
    clearSelection();
    await tagsStore.fetchTags();
    await search();
  }

  async function bulkAddLabel(ids: string[], labelName: string): Promise<void> {
    await tauriCommand('bulk_add_label_to_articles', { articleIds: ids, labelName });
    clearSelection();
    await labelsStore.fetchLabels();
    await search();
  }

  const hasReturnTarget = computed(
    () => returnToArticleId.value !== null || returnToReferencePaperId.value !== null
  );

  /** Navigate to an article while saving the current one as a return target. */
  async function navigateToArticle(targetId: string, fromReferencePaperId?: string): Promise<void> {
    // Skip if already viewing this article
    if (selectedArticle.value?.id === targetId) return;
    if (selectedArticle.value) {
      returnToArticleId.value = selectedArticle.value.id;
    }
    if (fromReferencePaperId) {
      returnToReferencePaperId.value = fromReferencePaperId;
    }
    await selectArticle(targetId);
  }

  function closeDetail(): void {
    if (returnToArticleId.value) {
      // Navigate back to the previous article instead of closing
      const returnId = returnToArticleId.value;
      returnToArticleId.value = null;
      void selectArticle(returnId);
      return;
    }
    if (returnToReferencePaperId.value) {
      returnToReferencePaperId.value = null;
    }
    showDetail.value = false;
    selectedArticle.value = null;
    auditTrail.value = [];
  }

  /**
   * Apply an initial filter state derived from route query parameters.
   * Sets the active status tab and/or tag/label/year/journal filters, then searches.
   *
   * Year and journal params are used by the Bibliometrics timeline deep-links
   * ("View articles from YYYY" and "View this journal's articles"). Both the
   * display `filter` and the search `query` are synced so the filter panel
   * reflects the active state.
   *
   * When `resetFilters` is true (decision D5), ALL existing `filter.*` +
   * `query.*` filter fields are cleared to defaults before the incoming params
   * are applied. This prevents the keep-alive-cached ArticleList from
   * overlaying stale filters on top of a fresh bibliometric deep-link (e.g.
   * landing on `author="Bob" AND yearFrom=2020` when the user clicked the
   * Co-Authorship Papers box while a year filter was still active). The biblio
   * metric summarized the included corpus with no extra filters, so the list
   * must reflect exactly the deep-link's filter. Sort (`sortBy`/`sortDir`) and
   * page-size are intentionally preserved (display preferences, not filters).
   * Status is reset by the incoming `status` param itself.
   */
  async function applyRouteParams(params: {
    status?: string;
    tags?: string[];
    labels?: string[];
    yearFrom?: number;
    yearTo?: number;
    journal?: string;
    author?: string;
    /** When true, keep the filter panel collapsed even though filters are applied. */
    filterCollapsed?: boolean;
    /**
     * When true, clear all existing `filter.*` + `query.*` filter fields to
     * defaults before applying the incoming params (decision D5). Used by the
     * bibliometric deep-links (`buildBiblioArticleQuery` envelope) so the
     * cached ArticleList does not overlay stale filters on the fresh
     * deep-link. Default `false` preserves the overlay behavior for existing
     * callers (dashboard, tag/label deep-links, status deep-links).
     */
    resetFilters?: boolean;
  }): Promise<void> {
    // Compute the filter-panel visibility flag once so every branch (tags,
    // labels, year, journal, author) honors `filterCollapsed` consistently.
    // When `filterCollapsed` is true (e.g. navigating from the Tags & Labels
    // screen or Bibliometrics deep-links), the panel stays collapsed even
    // though filters are applied.
    const showPanel = !params.filterCollapsed;

    // D5: optional reset-before-apply. Runs BEFORE the param-application
    // block so the incoming params overwrite the freshly-cleared defaults.
    // Mirrors the field list in `clearFilters` (minus the search() call,
    // which `applyRouteParams` performs at the end) plus the toolbar
    // `searchText` (so a stale toolbar search does not survive the reset).
    if (params.resetFilters) {
      filter.titleText = '';
      filter.authorText = '';
      filter.yearFrom = null;
      filter.yearTo = null;
      filter.journal = '';
      filter.doiText = '';
      filter.doiEmpty = false;
      filter.tags = [];
      filter.labels = [];
      filter.excludedTags = [];
      filter.excludedLabels = [];
      query.search = null;
      query.yearFrom = null;
      query.yearTo = null;
      query.author = null;
      query.journal = null;
      query.doi = null;
      query.doiEmpty = false;
      query.tags = [];
      query.labels = [];
      query.excludedTags = [];
      query.excludedLabels = [];
      searchText.value = '';
      resetPage();
    }

    if (params.status && STATUS_TABS.includes(params.status as StatusTab)) {
      activeStatusTab.value = params.status as StatusTab;
      if (params.status === 'error') {
        query.status = 'working';
        query.screeningErrorsOnly = true;
      } else {
        query.status = params.status === 'all' ? null : params.status;
        query.screeningErrorsOnly = false;
      }
    }
    if (params.tags && params.tags.length > 0) {
      // Resolve tag IDs to names for both display and query
      const tagNames = params.tags
        .map((id) => tagsStore.tags.find((t) => t.id === id)?.name)
        .filter((n): n is string => !!n);
      filter.tags = tagNames;
      query.tags = tagNames;
      if (showPanel) showFilters.value = true;
    }
    if (params.labels && params.labels.length > 0) {
      // Resolve label IDs to names for both display and query
      const labelNames = params.labels
        .map((id) => labelsStore.labels.find((l) => l.id === id)?.name)
        .filter((n): n is string => !!n);
      filter.labels = labelNames;
      query.labels = labelNames;
      if (showPanel) showFilters.value = true;
    }
    if (params.yearFrom !== undefined && Number.isFinite(params.yearFrom)) {
      filter.yearFrom = params.yearFrom;
      query.yearFrom = params.yearFrom;
      if (showPanel) showFilters.value = true;
    }
    if (params.yearTo !== undefined && Number.isFinite(params.yearTo)) {
      filter.yearTo = params.yearTo;
      query.yearTo = params.yearTo;
      if (showPanel) showFilters.value = true;
    }
    if (params.journal) {
      filter.journal = params.journal;
      query.journal = params.journal;
      if (showPanel) showFilters.value = true;
    }
    if (params.author) {
      filter.authorText = params.author;
      query.author = params.author;
      if (showPanel) showFilters.value = true;
    }
    await search();
  }

  return {
    articles,
    loading,
    error,
    query,
    selectedArticle,
    auditTrail,
    showDetail,
    activeStatusTab,
    showFilters,
    sortColumn,
    sortDirection,
    filter,
    statusCounts,
    allAuthors,
    allTags,
    allLabels,
    STATUS_TABS,
    search,
    fetchCounts,
    selectArticle,
    refreshArticle,
    moveArticle,
    updateNotes,
    updateTags,
    updateLabels,
    updateCriteria,
    updateMetadata,
    hasPrevious,
    hasNext,
    navigatePrev,
    navigateNext,
    closeDetail,
    setStatusTab,
    toggleSort,
    toggleFilters,
    applyFilters,
    clearFilters,
    applyRouteParams,
    pageSize,
    currentPage,
    totalPages,
    canGoPrev,
    canGoNext,
    goToPage,
    selectedGlobalIndex,
    searchText,
    activeTotalCount,
    isFiltered,
    resultCount,
    rangeStart,
    rangeEnd,
    changePageSize,
    executeToolbarSearch,
    clearSearch,
    hasReturnTarget,
    navigateToArticle,
    returnToReferencePaperId,
    // Multi-select
    selectedIds,
    selectedCount,
    allSelected,
    someSelected,
    toggleSelect,
    toggleSelectRange,
    toggleSelectAll,
    clearSelection,
    // Bulk operations
    bulkUpdateStatus,
    bulkAddTag,
    bulkAddLabel,
    // Full text
    attachFullText,
    deleteFullTextAttachment,
    readFullTextContent,
    getFullTextFilePath,
  };
}
