import { ref, reactive, computed, type Ref } from 'vue';
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
  /** Case-insensitive partial-match DOI. Mutually exclusive with `doiEmpty`. */
  doiText: string;
  /** Restrict to articles with no DOI. */
  doiEmpty: boolean;
  tags: string[];
  labels: string[];
  /** Tags the article must NOT have. Toggled from inclusion to exclusion in the filter panel. */
  excludedTags: string[];
  /** Labels the article must NOT have. */
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
  /** Partial-match DOI. Null/empty filters nothing. */
  doi: string | null;
  /** Restrict to articles with no DOI. */
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

/** Accepted route params for {@link applyRouteParams}. */
interface RouteParams {
  status?: string;
  tags?: string[];
  labels?: string[];
  yearFrom?: number;
  yearTo?: number;
  journal?: string;
  author?: string;
  /** Keep the filter panel collapsed even with filters applied (deep-links). */
  filterCollapsed?: boolean;
  /**
   * When true, clear all `filter.*` + `query.*` fields to defaults before
   * applying incoming params. Used by bibliometric deep-links.
   */
  resetFilters?: boolean;
}

/** Minimal structural shape the tag/label helpers need from a Pinia store. */
interface IdNameResolver {
  id: string;
  name: string;
}

/**
 * D5 reset: clear all filter + query fields to defaults before applying
 * incoming params. Runs BEFORE param-application so incoming params overwrite
 * the cleared defaults.
 */
function resetFilterState(
  filter: ArticleFilter,
  query: ArticleQuery,
  searchText: Ref<string>,
  resetPage: () => void
): void {
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

/**
 * Apply the `status` route param: resolve the active tab and set
 * `query.status` + `screeningErrorsOnly`. The `"error"` tab is
 * special-cased to `status='working'` + `screeningErrorsOnly=true`.
 * No-op when `status` is not a recognized {@link StatusTab} value.
 */
function applyStatusParam(
  status: string,
  activeStatusTab: Ref<StatusTab>,
  query: ArticleQuery
): void {
  if (!STATUS_TABS.includes(status as StatusTab)) return;
  activeStatusTab.value = status as StatusTab;
  if (status === 'error') {
    query.status = 'working';
    query.screeningErrorsOnly = true;
  } else {
    query.status = status === 'all' ? null : status;
    query.screeningErrorsOnly = false;
  }
}

/**
 * Apply `tags` + `labels` route params. Each ID is resolved to its display
 * name via the corresponding store. No-op when arrays are empty.
 */
function applyTagLabelParams(
  params: Pick<RouteParams, 'tags' | 'labels'>,
  filter: ArticleFilter,
  query: ArticleQuery,
  tagsStore: { tags: IdNameResolver[] },
  labelsStore: { labels: IdNameResolver[] },
  showPanel: boolean,
  showFilters: Ref<boolean>
): void {
  if (params.tags && params.tags.length > 0) {
    const tagNames = params.tags
      .map((id) => tagsStore.tags.find((t) => t.id === id)?.name)
      .filter((n): n is string => !!n);
    filter.tags = tagNames;
    query.tags = tagNames;
    if (showPanel) showFilters.value = true;
  }
  if (params.labels && params.labels.length > 0) {
    const labelNames = params.labels
      .map((id) => labelsStore.labels.find((l) => l.id === id)?.name)
      .filter((n): n is string => !!n);
    filter.labels = labelNames;
    query.labels = labelNames;
    if (showPanel) showFilters.value = true;
  }
}

/**
 * Apply `yearFrom`, `yearTo`, `journal`, and `author` route params.
 * Each is synced to both the display `filter` and the search `query`.
 */
function applyNumericAndTextParams(
  params: Pick<RouteParams, 'yearFrom' | 'yearTo' | 'journal' | 'author'>,
  filter: ArticleFilter,
  query: ArticleQuery,
  showPanel: boolean,
  showFilters: Ref<boolean>
): void {
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
}

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

  function applyFilters(): Promise<void> {
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
    return search().then(autoSelectSingleResult);
  }

  /**
   * Auto-open the detail panel when a filter application yields exactly one
   * result. No-op otherwise.
   */
  function autoSelectSingleResult(): Promise<void> {
    if (articles.value.length === 1) {
      const only = articles.value[0];
      if (only) {
        return selectArticle(only.id);
      }
    }
    return Promise.resolve();
  }

  /**
   * Gate for {@link autoSelectSingleResult}: only fire when the user
   * explicitly filtered by tag/label/year/journal/author. NOT for a bare
   * status-only deep-link.
   */
  function routeHasFilterDimensions(params: RouteParams): boolean {
    return Boolean(
      (params.tags && params.tags.length > 0) ||
      (params.labels && params.labels.length > 0) ||
      (params.yearFrom !== undefined && Number.isFinite(params.yearFrom)) ||
      (params.yearTo !== undefined && Number.isFinite(params.yearTo)) ||
      params.journal ||
      params.author
    );
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
   * article without going through the status-move path (e.g. AI screening,
   * AI summary, translation). Falls back to `selectArticle` if the article is
   * no longer in the list.
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

  /**
   * Permanently delete an article. The confirmation dialog is owned by
   * `article-detail-panel.vue`. Re-throws the backend error so the caller can
   * surface a toast.
   */
  async function deleteArticle(id: string): Promise<void> {
    await tauriCommand('delete_article', { id });
    // Remove the deleted article from the cached list so the table redraws
    // immediately without waiting for the search() round-trip.
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1);
    }
    // Close the detail panel: the selectedArticle no longer exists.
    selectedArticle.value = null;
    auditTrail.value = [];
    showDetail.value = false;
    returnToArticleId.value = null;
    returnToReferencePaperId.value = null;
    // Refresh counts in the background (tab badges + biblio/wiki flags).
    void fetchCounts();
    // Re-run the query so the page is consistent (e.g. a new article slides
    // in to fill the vacated slot when paginating).
    void search();
  }

  /** Patch the articles list row with the latest selectedArticle data. */
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
   * Update a single metadata field on an article via the in-place editor.
   * `field` is the snake_case DB column name (e.g. `"publication_year"`).
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

  /**
   * Clear AI reasoning text + confidence. Nulls `ai_decision`, `ai_reasoning`,
   * `ai_confidence`. `status`, `screened_at`, and `manual_override` are preserved.
   */
  async function clearAiReasoning(id: string): Promise<void> {
    await tauriCommand('clear_ai_reasoning', { id });
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

  /** @returns number of articles that actually received the tag. */
  async function bulkAddTag(ids: string[], tagName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_add_tag_to_articles', {
      articleIds: ids,
      tagName,
    });
    clearSelection();
    await tagsStore.fetchTags();
    await search();
    return affected;
  }

  /** @returns number of articles that actually received the label. */
  async function bulkAddLabel(ids: string[], labelName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_add_label_to_articles', {
      articleIds: ids,
      labelName,
    });
    clearSelection();
    await labelsStore.fetchLabels();
    await search();
    return affected;
  }

  /** @returns number of articles from which the tag was removed (0 = not present). */
  async function bulkRemoveTag(ids: string[], tagName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_remove_tag_from_articles', {
      articleIds: ids,
      tagName,
    });
    clearSelection();
    await tagsStore.fetchTags();
    await search();
    return affected;
  }

  /** @returns number of articles from which the label was removed. */
  async function bulkRemoveLabel(ids: string[], labelName: string): Promise<number> {
    const affected = await tauriCommand<number>('bulk_remove_label_from_articles', {
      articleIds: ids,
      labelName,
    });
    clearSelection();
    await labelsStore.fetchLabels();
    await search();
    return affected;
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
   * Apply route query params as article-list filters, then search.
   *
   * When `resetFilters` is true, ALL filter fields are cleared before
   * applying incoming params (keeps keep-alive-cached ArticleList from
   * overlaying stale filters on a bibliometric deep-link).
   */
  async function applyRouteParams(params: RouteParams): Promise<void> {
    /* Compute filter-panel visibility once so every helper honors
    `filterCollapsed`. When true, the panel stays collapsed even with applied
    filters. */
    const showPanel = !params.filterCollapsed;

    // D5: optional reset-before-apply. Runs BEFORE the param-application
    // helpers so the incoming params overwrite the freshly-cleared defaults.
    if (params.resetFilters) {
      resetFilterState(filter, query, searchText, resetPage);
      // Hard-close any open detail panel: the displayed article is from a
      // prior session and almost certainly does not match the fresh deep-link
      // filter. Hard-close (not `closeDetail()`, which would walk the
      // back-stack and re-open the previous article) and clear the back-stack
      // too so the reset is a clean slate. `autoSelectSingleResult` below can
      // still open the FRESH sole-result article.
      showDetail.value = false;
      selectedArticle.value = null;
      auditTrail.value = [];
      returnToArticleId.value = null;
      returnToReferencePaperId.value = null;
    }

    if (params.status) {
      applyStatusParam(params.status, activeStatusTab, query);
    }
    applyTagLabelParams(params, filter, query, tagsStore, labelsStore, showPanel, showFilters);
    applyNumericAndTextParams(params, filter, query, showPanel, showFilters);
    await search();
    /* Auto-open the detail panel when a deep-link with a filter dimension
    (tag/label/year/journal/author) yields exactly one result. Status-only
    deep-links just load the list. */
    if (routeHasFilterDimensions(params)) {
      await autoSelectSingleResult();
    }
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
    deleteArticle,
    updateNotes,
    updateTags,
    updateLabels,
    updateCriteria,
    updateMetadata,
    clearAiReasoning,
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
    bulkRemoveTag,
    bulkRemoveLabel,
    // Full text
    attachFullText,
    deleteFullTextAttachment,
    readFullTextContent,
    getFullTextFilePath,
  };
}
