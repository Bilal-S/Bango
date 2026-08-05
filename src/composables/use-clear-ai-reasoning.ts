import { useToast } from './use-toast';

/** Options for {@link useClearAiReasoning}. */
export interface ClearAiReasoningOptions {
  /** Low-level clear function (typically `clearAiReasoning` from `useArticleSearch`). */
  clearAiReasoning: (articleId: string) => Promise<void>;
}

/**
 * Shared UI orchestration for clearing AI reasoning after user confirmation.
 * Mirrors `useArticleDelete`. The confirmation dialog is owned by
 * `article-detail-panel.vue`.
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
