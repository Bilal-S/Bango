import { useToast } from './use-toast';

/**
 * Options for {@link useArticleDelete}.
 */
export interface ArticleDeleteOptions {
  /**
   * Low-level delete function (typically `deleteArticle` from
   * `useArticleSearch`, which wraps the `delete_article` IPC + removes the
   * row from the cached list + refreshes counts + closes the detail panel).
   * The composable handles the surrounding UI orchestration (toast +
   * post-delete hook) and delegates the actual delete to this function.
   */
  deleteArticle: (articleId: string) => Promise<void>;
  /**
   * Optional post-delete hook. Invoked only after a successful delete, after
   * the success toast. Used by `article-list.vue` and the biblio/chat/wiki
   * views to reset their local `isDetailFullScreen` flag (the panel is gone)
   * + the `showArticleDetail` gate. The hook is fire-and-forget (no return
   * value, errors are NOT propagated).
   */
  onDeleted?: () => void;
}

/**
 * Shared UI orchestration for permanently deleting an article after the user
 * confirms the in-panel dialog. Centralizes the delete -> toast -> post-hook
 * flow that would otherwise be duplicated across the four `ArticleDetailPanel`
 * host views (`article-list.vue`, `biblio-citations.vue`, `chat-view.vue`,
 * `wiki-view.vue`).
 *
 * The confirmation dialog is owned by `article-detail-panel.vue` (so every host
 * gets identical UX); this composable runs only after the user has confirmed.
 * The low-level IPC + list/panel teardown stays in `useArticleSearch.deleteArticle`;
 * this composable owns only the toast + post-delete hook shell.
 *
 * Mirrors `useFullTextAttachment` (same injectable-fn + onXxx-hook shape) so the
 * two detail-panel action handlers stay symmetric.
 *
 * Error toasts include the underlying message so all four hosts report delete
 * failures with equal detail.
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
