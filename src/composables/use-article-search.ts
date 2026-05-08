import { ref, reactive, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
import { useArticlesStore } from '@/stores/articles';
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
}

export type SortDirection = 'asc' | 'desc';

const STATUS_TABS: readonly (ArticleStatus | 'all')[] = [
  'all',
  'duplicate',
  'working',
  'included',
  'rejected',
] as const;

export type StatusTab = (typeof STATUS_TABS)[number];

export function useArticleSearch() {
  const articlesStore = useArticlesStore();

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
  });

  const statusCounts = ref<ArticleCounts>({
    // Seed from the pre-warmed store so counts render immediately
    // without waiting for the get_article_counts IPC round-trip.
    all: articlesStore.totalImported,
    duplicate: articlesStore.byStatus.duplicate,
    working: articlesStore.byStatus.working,
    included: articlesStore.byStatus.included,
    rejected: articlesStore.byStatus.rejected,
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
    const tagSet = new Set<string>();
    for (const article of articles.value) {
      for (const tag of article.tags) {
        tagSet.add(tag);
      }
    }
    return Array.from(tagSet).sort();
  });

  const allLabels = computed((): string[] => {
    const labelSet = new Set<string>();
    for (const article of articles.value) {
      for (const label of article.labels) {
        labelSet.add(label);
      }
    }
    return Array.from(labelSet).sort();
  });

  function setStatusTab(tab: StatusTab): void {
    activeStatusTab.value = tab;
    query.status = tab === 'all' ? null : tab;
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
    await selectArticle(id);
    await search();
  }

  async function updateNotes(id: string, notes: string): Promise<void> {
    await tauriCommand('update_article_notes', { id, notes });
    await selectArticle(id);
  }

  async function updateTags(id: string, tagIds: string[]): Promise<void> {
    await tauriCommand('update_article_tags', { id, tagIds });
    await selectArticle(id);
  }

  async function updateLabels(id: string, labelIds: string[]): Promise<void> {
    await tauriCommand('update_article_labels', { id, labelIds });
    await selectArticle(id);
  }

  function closeDetail(): void {
    showDetail.value = false;
    selectedArticle.value = null;
    auditTrail.value = [];
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
    closeDetail,
    setStatusTab,
    toggleSort,
    toggleFilters,
    applyFilters,
    clearFilters,
  };
}
