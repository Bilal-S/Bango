import { useToast } from './use-toast';

/** Options for {@link useArticleDelete}. */
export interface ArticleDeleteOptions {
  /** Low-level delete function (typically `deleteArticle` from `useArticleSearch`). */
  deleteArticle: (articleId: string) => Promise<void>;
  /** Optional post-delete hook, invoked after success toast. */
  onDeleted?: () => void;
}

/**
 * Shared UI orchestration for permanently deleting an article after user
 * confirmation. The confirmation dialog is owned by `article-detail-panel.vue`.
 * Mirrors `useFullTextAttachment` (injectable-fn + post-hook shape).
 */
export function useArticleDelete(options: ArticleDeleteOptions) {
  const { deleteArticle, onDeleted } = options;
  const toast = useToast();

  async function handleDeleteArticle(articleId: string): Promise<void> {
    try {
      await deleteArticle(articleId);
      toast.show('Article deleted.', 'success');
      onDeleted?.();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.show(`Failed to delete article: ${msg}`, 'error');
    }
  }

  return { handleDeleteArticle };
}
