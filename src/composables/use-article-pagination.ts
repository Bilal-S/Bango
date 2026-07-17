import { ref, computed, type Ref, type ComputedRef } from 'vue';
import type { Article, ArticleCounts } from '@/types';

/** Minimal shape of the query object needed by pagination. */
export interface PaginationQuery {
  offset: number;
  limit: number;
}

export interface PaginationDeps {
  articles: Ref<Article[]>;
  selectedArticle: Ref<Article | null>;
  query: PaginationQuery;
  statusCounts: Ref<ArticleCounts>;
  activeStatusTab: Ref<string>;
  /** Called whenever the page changes (triggers a backend query). */
  search: () => Promise<void>;
  /** Whether the current view is filtered (affects rangeStart/rangeEnd). */
  isFiltered: ComputedRef<boolean>;
  /** Total count for the active tab. */
  activeTotalCount: ComputedRef<number>;
}

export function useArticlePagination(deps: PaginationDeps) {
  const { articles, selectedArticle, query, search, isFiltered, activeTotalCount } = deps;

  const pageSize = ref(10);
  const currentPage = ref(1);

  /** Display count: filtered result length when filtering, tab total otherwise. */
  const resultCount = computed(() => {
    if (isFiltered.value) return articles.value.length;
    return activeTotalCount.value;
  });

  const totalPages = computed(() => {
    // When the view is filtered, the backend returns only the matching
    // articles (capped at `pageSize`), so the page count is driven by the
    // filtered result length, NOT the unfiltered tab total. Using
    // `activeTotalCount` here would over-report pages (e.g. "Page 1 of 3"
    // when only 5 articles match out of 25).
    const total = isFiltered.value ? resultCount.value : activeTotalCount.value;
    return Math.max(1, Math.ceil(total / pageSize.value));
  });

  const canGoPrev = computed(() => currentPage.value > 1);
  const canGoNext = computed(() => currentPage.value < totalPages.value);

  /** 1-based index of the selected article on the current page (-1 if none). */
  const selectedIndex = computed(() => {
    if (!selectedArticle.value) return -1;
    return articles.value.findIndex((a) => a.id === selectedArticle.value!.id);
  });

  /** 1-based global position of the selected article across all pages. */
  const selectedGlobalIndex = computed(() => {
    if (selectedIndex.value < 0) return 0;
    // When filtered, the loaded page IS the entire filtered result set (no
    // offset math); the position is the 1-based index within it. The
    // unfiltered branch keeps the multi-page global-position math.
    if (isFiltered.value) return selectedIndex.value + 1;
    return (currentPage.value - 1) * pageSize.value + selectedIndex.value + 1;
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

  function resetPage(): void {
    currentPage.value = 1;
    query.offset = 0;
  }

  function goToPage(page: number): void {
    currentPage.value = page;
    query.offset = (page - 1) * pageSize.value;
    void search();
  }

  /** Change page size and reset to page 1. */
  function changePageSize(size: number): void {
    pageSize.value = size;
    query.limit = size;
    resetPage();
    void search();
  }

  return {
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
  };
}
