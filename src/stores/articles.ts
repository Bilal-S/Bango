import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { Article, ArticleStatus } from '@/types';
import { tauriCommand } from '@/composables/use-tauri-command';

export const useArticlesStore = defineStore('articles', () => {
  const articles = ref<Article[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const byStatus = computed(() => {
    const counts: Record<ArticleStatus, number> = {
      imported: 0,
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

  async function fetchArticles(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      articles.value = await tauriCommand<Article[]>('get_articles');
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { articles, loading, error, byStatus, totalImported, fetchArticles };
});
