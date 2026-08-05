import { open } from '@tauri-apps/plugin-dialog';
import { useToast } from './use-toast';

/** Options for {@link useFullTextAttachment}. */
export interface FullTextAttachmentOptions {
  /** Low-level attach function (typically `attachFullText` from `useArticleSearch`). */
  attachFullText: (articleId: string, filePath: string) => Promise<void>;
  /** Optional post-attach hook (e.g. auto-summary LLM call). */
  onAttached?: (articleId: string) => void;
}

/**
 * Shared UI orchestration for attaching a full-text PDF/TXT file via the OS
 * file dialog. Centralizes file-dialog + toast flow across all host views.
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
