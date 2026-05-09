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

  const pageSize = 100;
  const currentPage = ref(1);

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
    limit: pageSize,
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
    resetPage();
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

  async function moveArticle(id: string, newStatus: string): Promise<void> {
    await tauriCommand('update_article_status', { id, newStatus });
    // Update the article in-place in the local list to avoid a full table redraw / scroll reset
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      const updated: Article = { ...articles.value[idx]!, status: newStatus as ArticleStatus };
      articles.value.splice(idx, 1, updated);
    }
    await selectArticle(id);
    // Refresh counts in the background (e.g. tab badges)
    void fetchCounts();
  }

  async function updateNotes(id: string, notes: string): Promise<void> {
    await tauriCommand('update_article_notes', { id, notes });
    await selectArticle(id);
  }

  async function updateTags(id: string, tagIds: string[]): Promise<void> {
    await tauriCommand('update_article_tags', { id, tagIds });
    await selectArticle(id);
    await tagsStore.fetchTags();
  }

  async function updateLabels(id: string, labelIds: string[]): Promise<void> {
    await tauriCommand('update_article_labels', { id, labelIds });
    await selectArticle(id);
    await labelsStore.fetchLabels();
  }

  const selectedIndex = computed(() => {
    if (!selectedArticle.value) return -1;
    return articles.value.findIndex((a) => a.id === selectedArticle.value!.id);
  });

  const hasPrevious = computed(() => selectedIndex.value > 0);
  const hasNext = computed(() => {
    const idx = selectedIndex.value;
    return idx >= 0 && idx < articles.value.length - 1;
  });

  async function navigatePrev(): Promise<void> {
    if (!hasPrevious.value) return;
    const prev = articles.value[selectedIndex.value - 1];
    if (prev) await selectArticle(prev.id);
  }

  async function navigateNext(): Promise<void> {
    if (!hasNext.value) return;
    const next = articles.value[selectedIndex.value + 1];
    if (next) await selectArticle(next.id);
  }

  function goToPage(page: number): void {
    currentPage.value = page;
    query.offset = (page - 1) * pageSize;
    void search();
  }

  const totalPages = computed(() => {
    const total = statusCounts.value.all;
    return Math.max(1, Math.ceil(total / pageSize));
  });

  const canGoPrev = computed(() => currentPage.value > 1);
  const canGoNext = computed(() => currentPage.value < totalPages.value);

  function closeDetail(): void {
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
  };
}
