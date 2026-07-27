import { useToast } from './use-toast';

/**
 * Options for {@link useClearAiReasoning}.
 */
export interface ClearAiReasoningOptions {
  /**
   * Low-level clear function (typically `clearAiReasoning` from
   * `useArticleSearch`, which wraps the `clear_ai_reasoning` IPC +
   * re-fetches the article so the card updates live). The composable handles
   * the surrounding UI orchestration (toast) and delegates the actual clear
   * to this function.
   */
  clearAiReasoning: (articleId: string) => Promise<void>;
}

/**
 * Shared UI orchestration for clearing the AI reasoning text + confidence
 * from an article after the user confirms the in-panel dialog. Centralizes
 * the clear -> toast flow that would otherwise be duplicated across the four
 * `ArticleDetailPanel` host views (`article-list.vue`, `biblio-citations.vue`,
 * `chat-view.vue`, `wiki-view.vue`).
 *
 * The confirmation dialog is owned by `article-detail-panel.vue` (so every host
 * gets identical UX); this composable runs only after the user has confirmed.
 * The low-level IPC + article refresh stays in `useArticleSearch.clearAiReasoning`;
 * this composable owns only the toast shell.
 *
 * Mirrors `useArticleDelete` (same injectable-fn shape) so the two
 * detail-panel destructive-action handlers stay symmetric. No `onCleared`
 * hook is needed (unlike delete) because the panel stays open after a clear;
 * the host's local flags do not need resetting.
 *
 * Error toasts include the underlying message so all four hosts report
 * failures with equal detail.
 */
export function useClearAiReasoning(options: ClearAiReasoningOptions) {
  const { clearAiReasoning } = options;
  const toast = useToast();

  async function handleClearAiReasoning(articleId: string): Promise<void> {
    try {
      await clearAiReasoning(articleId);
      toast.show('AI decision cleared.', 'success');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.show(`Failed to clear AI reasoning: ${msg}`, 'error');
    }
  }

  return { handleClearAiReasoning };
}
