import { open } from '@tauri-apps/plugin-dialog';
import { useToast } from './use-toast';

/**
 * Options for {@link useFullTextAttachment}.
 */
export interface FullTextAttachmentOptions {
  /**
   * Low-level attach function (typically `attachFullText` from
   * `useArticleSearch`, which wraps `useArticleFullText` -> IPC). The
   * composable handles the surrounding UI orchestration (file dialog +
   * toasts) and delegates the actual attach + refresh to this function.
   */
  attachFullText: (articleId: string, filePath: string) => Promise<void>;
  /**
   * Optional post-attach hook. Invoked only after a successful attach, after
   * the success toast. Used by `article-list.vue` to fire the auto-summary
   * LLM call when the `bango-full-text-summaries` localStorage flag is on.
   */
  onAttached?: (articleId: string) => void;
}

/**
 * Shared UI orchestration for attaching a full-text PDF/TXT file to an
 * article via the OS file dialog. Centralizes the open-dialog -> toast ->
 * attach -> toast flow that was previously duplicated (with subtle
 * divergence) across `article-list.vue`, `biblio-citations.vue`,
 * `chat-view.vue`, and `wiki-view.vue`.
 *
 * The low-level IPC + refresh logic stays in `useArticleFullText` /
 * `useArticleSearch`; this composable owns only the file-dialog + toast
 * shell. Callers destructure `handleAttachFullText` and bind it directly to
 * `@attach-full-text` on `ArticleDetailPanel`.
 *
 * Error toasts include the underlying message (the more informative variant
 * previously used only by `article-list.vue`) so all four views now report
 * attach failures with equal detail.
 */
export function useFullTextAttachment(options: FullTextAttachmentOptions) {
  const { attachFullText, onAttached } = options;
  const toast = useToast();

  async function handleAttachFullText(articleId: string): Promise<void> {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Documents', extensions: ['pdf', 'txt'] }],
      });
      if (!selected) return;
      toast.show('Importing full text\u2026', 'info');
      await attachFullText(articleId, selected);
      toast.show('Full text attached successfully.', 'success');
      onAttached?.(articleId);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.show(`Failed to attach full text: ${msg}`, 'error');
    }
  }

  return { handleAttachFullText };
}
