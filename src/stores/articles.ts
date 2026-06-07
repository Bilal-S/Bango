import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { Article, ArticleStatus } from '@/types';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

export const useArticlesStore = defineStore('articles', () => {
  const articles = ref<Article[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const initialized = ref(false);

  const byStatus = computed(() => {
    const counts: Record<ArticleStatus, number> = {
      duplicate: 0,
      working: 0,
      included: 0,
      rejected: 0,
    };
    for (const article of articles.value) {
      counts[article.status]++;
    }
    return counts;
  });

  const totalImported = computed(() => articles.value.length);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetchArticles();
  }

  async function fetchArticles(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      articles.value = await tauriCommand<Article[]>('get_articles');
      initialized.value = true;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    articles.value = [];
    initialized.value = false;
  }

  /** Re-fetch a single article from the backend and update it in the store array. */
  async function refreshArticle(id: string): Promise<void> {
    try {
      const updated = await tauriCommand<Article>('get_article', { id });
      const idx = articles.value.findIndex((a) => a.id === id);
      if (idx !== -1) {
        articles.value[idx] = updated;
      }
    } catch {
      // Silently ignore — the article list will be refreshed on next navigation
    }
  }

  return {
    articles,
    loading,
    error,
    byStatus,
    totalImported,
    initialized,
    fetchIfNeeded,
    fetchArticles,
    invalidate,
    refreshArticle,
  };
});
