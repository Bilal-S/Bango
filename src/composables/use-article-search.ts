import { ref, reactive, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { Article, AuditEntry, ArticleStatus } from '@/types';

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
}

export type SortDirection = 'asc' | 'desc';

const STATUS_TABS: readonly (ArticleStatus | 'all')[] = [
  'all',
  'imported',
  'working',
  'included',
  'rejected',
] as const;

export type StatusTab = (typeof STATUS_TABS)[number];

export function useArticleSearch() {
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
  });

  const statusCounts = computed(() => {
    const counts: Record<string, number> = {
      all: articles.value.length,
      imported: 0,
      working: 0,
      included: 0,
      rejected: 0,
    };
    for (const article of articles.value) {
      counts[article.status] = (counts[article.status] ?? 0) + 1;
    }
    return counts;
  });

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
  }

  function toggleFilters(): void {
    showFilters.value = !showFilters.value;
  }

  function applyFilters(): void {
    query.search = filter.titleText || null;
    query.yearFrom = filter.yearFrom;
    query.yearTo = filter.yearTo;
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
    void search();
  }

  async function search(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      articles.value = await tauriCommand<Article[]>('query_articles', { query });
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
    closeDetail,
    setStatusTab,
    toggleSort,
    toggleFilters,
    applyFilters,
    clearFilters,
  };
}
