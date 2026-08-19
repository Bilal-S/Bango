import { computed, type ComputedRef, type Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { Article, AuditEntry } from '@/types';

export interface ArticleDetailDeps {
  articles: Ref<Article[]>;
  selectedArticle: Ref<Article | null>;
  auditTrail: Ref<AuditEntry[]>;
  showDetail: Ref<boolean>;
  returnToArticleId: Ref<string | null>;
  returnToReferencePaperId: Ref<string | null>;
  error: Ref<string | null>;
  /** 0-based index of the selected article on the current page (-1 if none). */
  selectedIndex: ComputedRef<number>;
  currentPage: Ref<number>;
  pageSize: Ref<number>;
  totalPages: ComputedRef<number>;
  query: { offset: number };
  search: () => Promise<void>;
  fetchCounts: () => Promise<void>;
}

/**
 * Article detail-panel state machine: selection + audit trail loading,
 * prev/next navigation (same page and cross-page), and the return-target
 * back-stack for article-to-article navigation. Extracted from
 * `useArticleSearch` (refactor1 T4.1); the parent re-exposes everything
 * unchanged. The state refs stay owned by the parent because pagination
 * (created before this composable) also reads `selectedArticle`.
 */
export function useArticleDetail(deps: ArticleDetailDeps) {
  const {
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
  } = deps;

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

  /** Patch the articles list row with the latest selectedArticle data. */
  function syncArticleToList(id: string): void {
    if (!selectedArticle.value || selectedArticle.value.id !== id) return;
    const idx = articles.value.findIndex((a) => a.id === id);
    if (idx >= 0) {
      articles.value.splice(idx, 1, { ...selectedArticle.value });
    }
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
   * Hard-close the detail panel without walking the back-stack (unlike
   * `closeDetail`, which re-opens the previous article). Used by the D5
   * reset-filters deep-link path and post-delete teardown so the reset is a
   * clean slate.
   */
  function resetDetailView(): void {
    showDetail.value = false;
    selectedArticle.value = null;
    auditTrail.value = [];
    returnToArticleId.value = null;
    returnToReferencePaperId.value = null;
  }

  return {
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
  };
}
