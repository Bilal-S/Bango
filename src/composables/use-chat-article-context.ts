/**
 * Article-context selection for the Chat view: the loaded article list
 * (duplicates filtered), the selected-context set backed by the chat store,
 * and the selector-modal open state. The selector modal itself owns its
 * search filter; the pills bar owns its tooltip.
 */

import { ref, computed, type Ref } from 'vue';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useToast } from '@/composables/use-toast';
import type { Article } from '@/types';

export function useChatArticleContext(args: {
  /** The chat store's selected-article id list (mutated through actions). */
  selectedArticleIds: Ref<string[]>;
  /** Adds an article id to the context set. */
  addSelectedArticle: (id: string) => void;
  /** Removes an article id from the context set. */
  removeSelectedArticle: (id: string) => void;
}) {
  const toast = useToast();

  /** All non-duplicate library articles (candidates for context). */
  const articles = ref<Article[]>([]);

  /** Whether the article selector modal is open. */
  const showSelector = ref(false);

  /** The selected (context) articles, resolved against the loaded list. */
  const selectedArticles = computed(() => {
    return articles.value.filter((a) => args.selectedArticleIds.value.includes(a.id));
  });

  /** Load the library (duplicates filtered out of the candidate list). */
  async function loadArticles(): Promise<void> {
    try {
      const all = await tauriCommand<Article[]>('get_articles');
      articles.value = all.filter((a) => a.status !== 'duplicate' && !a.duplicateOf);
    } catch {
      toast.show('Failed to load articles list', 'error');
    }
  }

  /** Toggle one article in/out of the context set. */
  function toggleArticleSelection(id: string): void {
    if (args.selectedArticleIds.value.includes(id)) {
      args.removeSelectedArticle(id);
    } else {
      args.addSelectedArticle(id);
    }
  }

  return {
    articles,
    showSelector,
    selectedArticles,
    loadArticles,
    toggleArticleSelection,
  };
}
