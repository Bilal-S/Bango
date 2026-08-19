import { ref, reactive, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
import { useArticlePagination } from './use-article-pagination';
import { useArticleSelection } from './use-article-selection';
import { useArticleFullText } from './use-article-full-text';
import {
  STATUS_TABS,
  createDefaultFilter,
  createDefaultQuery,
  isQueryFiltered,
  useArticleFilters,
} from './use-article-filters';
import type { SortDirection, StatusTab } from './use-article-filters';
import { useArticleDetail } from './use-article-detail';
import { useArticleMutations } from './use-article-mutations';
import { useArticleBulk } from './use-article-bulk';
import { useArticleRouteParams } from './use-article-route-params';
import { useArticleCounts } from './use-article-counts';
import { useArticlesStore } from '@/stores/articles';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import type { Article, AuditEntry } from '@/types';

export type { ArticleFilter, TitleMatchType } from './use-article-filters';

/**
 * Root article-list composable. Owns the shared reactive state (list, query,
 * status counts, detail-panel refs) and wires the extracted sub-composables:
 *
 * - `useArticleSelection` - multi-select set (checkbox column)
 * - `useArticlePagination` - page/page-size/offset math + range display
 * - `useArticleCounts` - status-tab counts + author/tag/label suggestions
 * - `useArticleDetail` - detail panel, prev/next nav, return-target stack
 * - `useArticleFilters` - status tabs, sort, filter panel, toolbar search
 * - `useArticleMutations` - single-article IPC mutations
 * - `useArticleBulk` - multi-select bulk IPC mutations
 * - `useArticleRouteParams` - deep-link (`?status=…&tags=…`) application
 * - `useArticleFullText` - full-text attachment IPC
 *
 * The returned object shape is a frozen contract consumed by
 * `article-list.vue`, `wiki-view.vue`, `chat-view.vue`, and the biblio views -
 * change internals only, never the shape (refactor1 plan §7).
 */
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

  const filter = reactive(createDefaultFilter());

  const searchText = ref('');

  const query = reactive(createDefaultQuery(defaultTab));

  // ── Status-tab counts + suggestion lists (extracted composable) ──────
  const { statusCounts, fetchCounts, activeTotalCount, allAuthors, allTags, allLabels } =
    useArticleCounts({
      articles,
      tagsStore,
      labelsStore,
      articlesStore,
      activeStatusTab,
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
  const isFiltered = computed(() => isQueryFiltered(query));

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

  // ── Detail panel + prev/next navigation (extracted composable) ──────
  const {
    selectArticle,
    refreshArticle,
    syncArticleToList,
    autoSelectSingleResult,
    hasPrevious,
    hasNext,
    navigatePrev,
    navigateNext,
    hasReturnTarget,
    navigateToArticle,
    closeDetail,
    resetDetailView,
  } = useArticleDetail({
    articles,
    selectedArticle,
    auditTrail,
    showDetail,
    returnToArticleId,
    returnToReferencePaperId,
    error,
    selectedIndex,
    currentPage,
    pageSize,
    totalPages,
    query,
    search,
    fetchCounts,
  });

  // ── Tabs / sort / filter panel / toolbar search (extracted) ─────────
  const {
    setStatusTab,
    toggleSort,
    toggleFilters,
    applyFilters,
    clearFilters,
    executeToolbarSearch,
    clearSearch,
  } = useArticleFilters({
    filter,
    query,
    searchText,
    activeStatusTab,
    showFilters,
    sortColumn,
    sortDirection,
    currentPage,
    pageSize,
    resetPage,
    search,
    autoSelectSingleResult,
  });

  // ── Single-article mutations (extracted composable) ─────────────────
  const {
    moveArticle,
    deleteArticle,
    updateNotes,
    updateMetadata,
    updateTags,
    updateLabels,
    updateCriteria,
    clearAiReasoning,
  } = useArticleMutations({
    articles,
    selectedArticle,
    auditTrail,
    hasNext,
    navigateNext,
    selectArticle,
    syncArticleToList,
    resetDetailView,
    fetchCounts,
    search,
    fetchTags: () => tagsStore.fetchTags(),
    fetchLabels: () => labelsStore.fetchLabels(),
  });

  // ── Bulk operations (extracted composable) ──────────────────────────
  const { bulkUpdateStatus, bulkAddTag, bulkAddLabel, bulkRemoveTag, bulkRemoveLabel } =
    useArticleBulk({
      clearSelection,
      search,
      fetchTags: () => tagsStore.fetchTags(),
      fetchLabels: () => labelsStore.fetchLabels(),
    });

  // ── Route deep-link application (extracted composable) ──────────────
  const { applyRouteParams } = useArticleRouteParams({
    filter,
    query,
    searchText,
    activeStatusTab,
    showFilters,
    tagsStore,
    labelsStore,
    resetPage,
    search,
    autoSelectSingleResult,
    resetDetailView,
  });

  // ── Full text (extracted composable) ─────────────────────────────
  const { attachFullText, deleteFullTextAttachment, readFullTextContent, getFullTextFilePath } =
    useArticleFullText({ selectArticle, syncArticleToList, fetchCounts });

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
