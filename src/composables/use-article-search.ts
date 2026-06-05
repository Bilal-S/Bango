import { ref, reactive, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
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
  tags: string[];
  labels: string[];
}

export interface ArticleQuery {
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
  tags: string[];
  labels: string[];
  limit: number;
  offset: number;
}

export type SortDirection = 'asc' | 'desc';

const STATUS_TABS: readonly (ArticleStatus | 'all' | 'error')[] = [
  'all',
  'duplicate',
  'working',
  'included',
  'rejected',
  'error',
] as const;

export type StatusTab = (typeof STATUS_TABS)[number];

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

  // Multi-select state for batch operations
  const selectedIds = ref<Set<string>>(new Set());

  const activeStatusTab = ref<StatusTab>('all');
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
    tags: [],
    labels: [],
  });

  const pageSize = ref(10);
  const currentPage = ref(1);
  const searchText = ref('');

  const query = reactive<ArticleQuery>({
    status: null,
    search: null,
    sortBy: null,
    sortDir: null,
    yearFrom: null,
    yearTo: null,
    manualOverrideOnly: false,
    screeningErrorsOnly: false,
    author: null,
    journal: null,
    tags: [],
    labels: [],
    limit: pageSize.value,
    offset: 0,
  });

  const statusCounts = ref<ArticleCounts>({
    // Seed from the pre-warmed store so counts render immediately
    // without waiting for the get_article_counts IPC round-trip.
    all: articlesStore.totalImported,
    duplicate: articlesStore.byStatus.duplicate,
    working: articlesStore.byStatus.working,
    included: articlesStore.byStatus.included,
    rejected: articlesStore.byStatus.rejected,
    error: 0,
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

  function resetPage(): void {
    currentPage.value = 1;
    query.offset = 0;
  }

  function setStatusTab(tab: StatusTab): void {
    activeStatusTab.value = tab;
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
    // Keep the current page — re-sort in-place so the user sees the
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
    query.tags = [...filter.tags];
    query.labels = [...filter.labels];
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
    filter.tags = [];
    filter.labels = [];
    query.search = null;
    query.yearFrom = null;
    query.yearTo = null;
    query.author = null;
    query.journal = null;
    query.tags = [];
    query.labels = [];
    resetPage();
    void search();
  }

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

  async function selectArticle(id: string): Promise<void> {
    try {
      selectedArticle.value = await tauriCommand<Article>('get_article', { id });
      auditTrail.value = await tauriCommand<AuditEntry[]>('get_audit_trail', { articleId: id });
      showDetail.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    }
  }

  async function moveArticle(id: string, newStatus: string): Promise<{ isLast: boolean }> {
    await tauriCommand('update_article_status', { id, newStatus });
    // Re-fetch the article so we get the updated changedAt from the backend
    const fresh = await tauriCommand<Article>('get_article', { id });
    // Patch the article in-place to reflect new status + changedAt without a full redraw
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1, fresh);
    }
    const isLast = !hasNext.value;
    if (!isLast) {
      await navigateNext();
    } else {
      selectedArticle.value = fresh;
      auditTrail.value = await tauriCommand<AuditEntry[]>('get_audit_trail', { articleId: id });
    }
    // Refresh counts in the background (e.g. tab badges)
    void fetchCounts();
    return { isLast };
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

  const selectedIndex = computed(() => {
    if (!selectedArticle.value) return -1;
    return articles.value.findIndex((a) => a.id === selectedArticle.value!.id);
  });

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
      // Cross to previous page — load it and select the last article
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
      // Cross to next page — load it and select the first article
      const nextPage = currentPage.value + 1;
      currentPage.value = nextPage;
      query.offset = (nextPage - 1) * pageSize.value;
      await search();
      const firstOnNextPage = articles.value[0];
      if (firstOnNextPage) await selectArticle(firstOnNextPage.id);
    }
  }

  /** 1-based global position of the selected article across all pages. */
  const selectedGlobalIndex = computed(() => {
    if (selectedIndex.value < 0) return 0;
    return (currentPage.value - 1) * pageSize.value + selectedIndex.value + 1;
  });

  /** Total article count for the currently active status tab. */
  const activeTotalCount = computed(() => {
    const tab = activeStatusTab.value;
    if (tab === 'all') return statusCounts.value.all;
    if (tab === 'error') return statusCounts.value.error;
    return statusCounts.value[tab as ArticleStatus] ?? 0;
  });

  /** Whether a search or filter is currently active. */
  const isFiltered = computed(() => {
    return !!(
      query.search ||
      query.author ||
      query.journal ||
      query.tags.length > 0 ||
      query.labels.length > 0 ||
      query.yearFrom !== null ||
      query.yearTo !== null
    );
  });

  /** Display count: filtered result length when filtering, tab total otherwise. */
  const resultCount = computed(() => {
    if (isFiltered.value) return articles.value.length;
    return activeTotalCount.value;
  });

  /** 1-based index of the first displayed article on the current page. */
  const rangeStart = computed(() => {
    if (resultCount.value === 0) return 0;
    if (isFiltered.value) return 1;
    return (currentPage.value - 1) * pageSize.value + 1;
  });

  /** 1-based index of the last displayed article on the current page. */
  const rangeEnd = computed(() => {
    if (isFiltered.value) return articles.value.length;
    return Math.min(currentPage.value * pageSize.value, activeTotalCount.value);
  });

  function goToPage(page: number): void {
    currentPage.value = page;
    query.offset = (page - 1) * pageSize.value;
    void search();
  }

  const totalPages = computed(() => {
    const total = activeTotalCount.value;
    return Math.max(1, Math.ceil(total / pageSize.value));
  });

  const canGoPrev = computed(() => currentPage.value > 1);
  const canGoNext = computed(() => currentPage.value < totalPages.value);

  /** Change page size and reset to page 1. */
  function changePageSize(size: number): void {
    pageSize.value = size;
    query.limit = size;
    resetPage();
    void search();
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

  // ── Multi-select helpers ──────────────────────────────────────────
  const selectedCount = computed(() => selectedIds.value.size);

  const allSelected = computed(
    () => articles.value.length > 0 && selectedIds.value.size === articles.value.length
  );

  const someSelected = computed(() => selectedIds.value.size > 0 && !allSelected.value);

  function toggleSelect(id: string): void {
    const s = new Set(selectedIds.value);
    if (s.has(id)) {
      s.delete(id);
    } else {
      s.add(id);
    }
    selectedIds.value = s;
  }

  function toggleSelectAll(): void {
    if (allSelected.value) {
      selectedIds.value = new Set();
    } else {
      selectedIds.value = new Set(articles.value.map((a) => a.id));
    }
  }

  function clearSelection(): void {
    selectedIds.value = new Set();
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

  const hasReturnTarget = computed(() => returnToArticleId.value !== null);

  /** Navigate to an article while saving the current one as a return target. */
  async function navigateToArticle(targetId: string): Promise<void> {
    if (selectedArticle.value) {
      returnToArticleId.value = selectedArticle.value.id;
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
    showDetail.value = false;
    selectedArticle.value = null;
    auditTrail.value = [];
  }

  /**
   * Apply an initial filter state derived from route query parameters.
   * Sets the active status tab and/or tag/label filters, then searches.
   */
  async function applyRouteParams(params: {
    status?: string;
    tags?: string[];
    labels?: string[];
  }): Promise<void> {
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
      showFilters.value = true;
    }
    if (params.labels && params.labels.length > 0) {
      // Resolve label IDs to names for both display and query
      const labelNames = params.labels
        .map((id) => labelsStore.labels.find((l) => l.id === id)?.name)
        .filter((n): n is string => !!n);
      filter.labels = labelNames;
      query.labels = labelNames;
      showFilters.value = true;
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
    selectArticle,
    moveArticle,
    updateNotes,
    updateTags,
    updateLabels,
    updateCriteria,
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
    // Multi-select
    selectedIds,
    selectedCount,
    allSelected,
    someSelected,
    toggleSelect,
    toggleSelectAll,
    clearSelection,
    // Bulk operations
    bulkUpdateStatus,
    bulkAddTag,
    bulkAddLabel,
  };
}
