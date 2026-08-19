import { computed, ref, type Ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { Article, ArticleCounts, ArticleStatus } from '@/types';

/** Minimal shape `useArticleCounts` needs from the articles Pinia store. */
interface ArticlesStatusStore {
  totalImported: number;
  byStatus: { duplicate: number; working: number; included: number; rejected: number };
}

export interface ArticleCountsDeps {
  articles: Ref<Article[]>;
  tagsStore: { tags: { name: string }[] };
  labelsStore: { labels: { name: string }[] };
  articlesStore: ArticlesStatusStore;
  /** Active status tab - selects which badge count is the tab total. */
  activeStatusTab: Ref<string>;
}

/**
 * Status-tab counts + suggestion lists for the Articles view (refactor1 T4.4):
 * seeds `statusCounts` from the pre-warmed articles store so tab badges render
 * immediately, refreshes them via the `get_article_counts` IPC (non-fatal on
 * error), and derives the author/tag/label suggestion lists. Extracted from
 * `useArticleSearch`; the parent re-exposes everything unchanged.
 */
export function useArticleCounts(deps: ArticleCountsDeps) {
  const { articles, tagsStore, labelsStore, articlesStore, activeStatusTab } = deps;

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

  /** Total count for the active tab (the number the toolbar range shows). */
  const activeTotalCount = computed(() => {
    const tab = activeStatusTab.value;
    if (tab === 'all') return statusCounts.value.all;
    if (tab === 'error') return statusCounts.value.error;
    return statusCounts.value[tab as ArticleStatus] ?? 0;
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
    return tagsStore.tags.map((t) => t.name).sort();
  });

  const allLabels = computed((): string[] => {
    return labelsStore.labels.map((l) => l.name).sort();
  });

  return { statusCounts, fetchCounts, activeTotalCount, allAuthors, allTags, allLabels };
}
