import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useToast } from './use-toast';

/**
 * Translation queue UI orchestration (language-plan-v2 Phase 5).
 *
 * Owns the confirmation-dialog state, the Tauri `enqueueArticleTranslation`
 * invoke, the immediate toast feedback, and the global `translation:complete`
 * event listener that refreshes the article + shows a success/error toast.
 *
 * Mirrors the `use-ai-summary.ts` pattern: the composable holds shared
 * singleton state so any host component (article-detail-panel, article-list,
 * biblio-citations, chat-view, wiki-view) can trigger a translation with one
 * call and the toasts + event-driven refresh work uniformly.
 */

/** Article IDs with a translation currently queued or running. */
const pendingTranslations = ref<Set<string>>(new Set());

let listenersInitialized = false;
let unlistenComplete: UnlistenFn | null = null;

/** Payload emitted by the Rust worker on `translation:complete`. */
interface TranslationCompletePayload {
  articleId: string;
  success: boolean;
  error?: string;
}

/** Lazily register the global Tauri event listener (once only). */
async function ensureGlobalListener(
  oncomplete: (articleId: string, success: boolean, error?: string) => void
): Promise<void> {
  if (listenersInitialized) return;
  listenersInitialized = true;

  unlistenComplete = await listen<TranslationCompletePayload>(
    'translation:complete',
    async (event) => {
      const { articleId, success, error } = event.payload;
      pendingTranslations.value.delete(articleId);
      const { show } = useToast();
      if (success) {
        show('Article translated to English.', 'success');
      } else {
        show(`Translation failed: ${error ?? 'unknown error'}`, 'error');
      }
      oncomplete(articleId, success, error);
    }
  );
}

export interface UseTranslationOptions {
  /** Called when a translation completes (success or failure) so the host
   * can refresh its article state. */
  onTranslationComplete?: (articleId: string, success: boolean, error?: string) => void;
}

export function useTranslation(options: UseTranslationOptions = {}) {
  const { show } = useToast();

  // Whether the confirmation dialog is open for the currently-selected article.
  const showTranslateDialog = ref(false);
  const translateArticleId = ref<string | null>(null);
  const translateArticleTitle = ref<string>('');

  // Register the global listener with the host's refresh callback.
  void ensureGlobalListener((articleId, success, error) => {
    options.onTranslationComplete?.(articleId, success, error);
  });

  /** Open the confirmation dialog for an article. */
  function requestTranslation(articleId: string, articleTitle: string): void {
    translateArticleId.value = articleId;
    translateArticleTitle.value = articleTitle;
    showTranslateDialog.value = true;
  }

  /** Confirm: enqueue the translation job via the Tauri command. */
  async function confirmTranslation(): Promise<void> {
    const articleId = translateArticleId.value;
    if (!articleId) return;
    showTranslateDialog.value = false;
    pendingTranslations.value.add(articleId);
    try {
      await invoke<boolean>('enqueue_article_translation', {
        articleId,
        triggerSource: 'manual',
      });
      show(`Translation queued for: ${translateArticleTitle.value}`, 'info');
    } catch (e) {
      pendingTranslations.value.delete(articleId);
      show(`Failed to queue translation: ${(e as Error).message ?? e}`, 'error');
    }
  }

  /** Cancel the confirmation dialog. */
  function cancelTranslation(): void {
    showTranslateDialog.value = false;
    translateArticleId.value = null;
  }

  /** Whether a translation is pending for the given article. */
  function isTranslationPending(articleId: string): boolean {
    return pendingTranslations.value.has(articleId);
  }

  return {
    showTranslateDialog,
    translateArticleTitle,
    requestTranslation,
    confirmTranslation,
    cancelTranslation,
    isTranslationPending,
  };
}

/** Test-only helper to reset the singleton state between unit tests. */
export function _resetTranslationStateForTests(): void {
  pendingTranslations.value = new Set();
  listenersInitialized = false;
  if (unlistenComplete) {
    unlistenComplete();
    unlistenComplete = null;
  }
}
