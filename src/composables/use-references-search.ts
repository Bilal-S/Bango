import { ref, computed } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { ReferencePaperQuery, LinkedArticleInfo } from '@/types';

const PAGE_SIZE = 25;

type MatchStatusFilter = 'all' | 'unmatched' | 'matched' | 'imported';

export function useReferencesSearch() {
  const searchText = ref('');
  const statusFilter = ref<MatchStatusFilter>('all');
  const papers = ref<ReferencePaperQuery[]>([]);
  const articlesOfInterest = ref<ReferencePaperQuery[]>([]);
  const loading = ref(false);
  const total = ref(0);
  const currentPage = ref(1);
  const error = ref<string | null>(null);

  // Linked articles cache (keyed by paper id)
  const linkedArticlesMap = ref<Record<string, LinkedArticleInfo[]>>({});
  const linkedArticlesLoading = ref<Record<string, boolean>>({});

  const totalPages = computed(() => Math.max(1, Math.ceil(total.value / PAGE_SIZE)));
  const canGoPrev = computed(() => currentPage.value > 1);
  const canGoNext = computed(() => currentPage.value < totalPages.value);

  async function search(term?: string): Promise<void> {
    if (term !== undefined) {
      searchText.value = term;
    }
    currentPage.value = 1;
    await loadPage();
  }

  async function loadPage(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const offset = (currentPage.value - 1) * PAGE_SIZE;
      const term = searchText.value.trim() || undefined;
      const matchStatus = statusFilter.value === 'all' ? null : statusFilter.value;
      const result = await tauriCommand<import('@/types').ReferencePaperQueryResult>(
        'query_reference_papers',
        { search: term ?? null, matchStatus, limit: PAGE_SIZE, offset }
      );
      papers.value = result.papers;
      total.value = result.total;
    } catch (e: unknown) {
      console.error('[references-search] loadPage failed:', e);
      error.value = e instanceof Error ? e.message : String(e);
      papers.value = [];
      total.value = 0;
    } finally {
      loading.value = false;
    }
  }

  async function goToPage(page: number): Promise<void> {
    currentPage.value = Math.max(1, Math.min(page, totalPages.value));
    await loadPage();
  }

  async function refresh(): Promise<void> {
    await loadPage();
  }

  async function loadArticlesOfInterest(): Promise<void> {
    try {
      const result = await tauriCommand<ReferencePaperQuery[]>(
        'get_reference_articles_of_interest',
        {}
      );
      articlesOfInterest.value = result;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      articlesOfInterest.value = [];
    }
  }

  async function loadLinkedArticles(paperId: string): Promise<LinkedArticleInfo[]> {
    if (linkedArticlesMap.value[paperId]) {
      return linkedArticlesMap.value[paperId];
    }
    linkedArticlesLoading.value[paperId] = true;
    try {
      const result = await tauriCommand<LinkedArticleInfo[]>('get_linked_articles_for_paper', {
        paperId,
      });
      linkedArticlesMap.value[paperId] = result;
      return result;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return [];
    } finally {
      linkedArticlesLoading.value[paperId] = false;
    }
  }

  /**
   * Promote a reference paper to a Working-list article.
   *
   * @param paperId - The reference paper id to promote.
   * @param options.refreshArticlesOfInterest - When `false`, skips the
   *   `loadArticlesOfInterest()` refresh so the caller can remove the card
   *   locally with an exit animation instead of letting the refresh yank it.
   *   Defaults to `true` (backward compatible).
   * @returns The new article id, or `null` on failure.
   */
  async function promotePaper(
    paperId: string,
    options: { refreshArticlesOfInterest?: boolean } = {}
  ): Promise<string | null> {
    const { refreshArticlesOfInterest = true } = options;
    try {
      const result = await tauriCommand<{ articleId: string; articleTitle: string }>(
        'promote_reference_to_article',
        { referencePaperId: paperId }
      );
      // Refresh the main papers list in both modes. The articles-of-interest
      // refresh is skippable so the caller can animate the card out locally.
      const tasks: Promise<unknown>[] = [loadPage()];
      if (refreshArticlesOfInterest) tasks.push(loadArticlesOfInterest());
      await Promise.all(tasks);
      return result.articleId;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    }
  }

  return {
    searchText,
    statusFilter,
    papers,
    articlesOfInterest,
    loading,
    total,
    currentPage,
    totalPages,
    canGoPrev,
    canGoNext,
    error,
    linkedArticlesMap,
    linkedArticlesLoading,
    search,
    loadPage,
    goToPage,
    refresh,
    loadArticlesOfInterest,
    loadLinkedArticles,
    promotePaper,
  };
}
