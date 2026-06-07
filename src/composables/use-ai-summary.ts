import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useToast } from './use-toast';

export interface AiSummaryData {
  field: string;
  subfield: string;
  structured_extraction: Record<string, string>;
  summary_150_250_words: string;
  key_insights: string[];
  keywords: string[];
}

export const pendingSummaries = ref<Set<string>>(new Set());

/**
 * Request an AI summary for an article's full text.
 * Shows a toast immediately and processes asynchronously.
 */
export async function requestArticleAiSummary(articleId: string, articleTitle: string) {
  const { show } = useToast();
  try {
    show('Submitted for AI summary', 'info');
    pendingSummaries.value.add(articleId);
    // Fire-and-forget: the command is async and emits an event on success.
    invoke<string>('generate_article_ai_summary', { articleId }).catch((e: unknown) => {
      pendingSummaries.value.delete(articleId);
      const msg = e instanceof Error ? e.message : String(e);
      show(`AI summary failed: ${msg}`, 'error');
    });
  } catch (e) {
    pendingSummaries.value.delete(articleId);
    const msg = e instanceof Error ? e.message : String(e);
    show(`AI summary failed: ${msg}`, 'error');
  }
  // Suppress unused variable warning
  void articleTitle;
}

/**
 * Parse a raw AI summary JSON string into a structured object.
 */
export function parseAiSummary(raw: string | null | undefined): AiSummaryData | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && parsed.summary_150_250_words) {
      return parsed as AiSummaryData;
    }
    return null;
  } catch {
    return null;
  }
}

/**
 * Composable to listen for AI summary completion and error events.
 * @param onSummaryComplete - Optional callback invoked with the articleId when
 *   a summary completes. Use this to refresh the selected article in the view.
 */
export function useAiSummaryEvents(onSummaryComplete?: (articleId: string) => Promise<void>) {
  let unlistenSuccess: UnlistenFn | null = null;
  let unlistenError: UnlistenFn | null = null;

  onMounted(async () => {
    unlistenSuccess = await listen<{ articleId: string; title: string }>(
      'article-ai-summary-complete',
      async (event) => {
        pendingSummaries.value.delete(event.payload.articleId);
        const { show } = useToast();
        show(`Summary complete for: ${event.payload.title}`, 'success');
        // Let the view refresh the selected article so detail panel updates live
        if (onSummaryComplete) {
          await onSummaryComplete(event.payload.articleId);
        }
      }
    );

    unlistenError = await listen<{ articleId: string; error: string }>(
      'article-ai-summary-error',
      (event) => {
        pendingSummaries.value.delete(event.payload.articleId);
        const { show } = useToast();
        show(`AI summary failed: ${event.payload.error}`, 'error');
      }
    );
  });

  onUnmounted(() => {
    unlistenSuccess?.();
    unlistenError?.();
  });

  return { pendingSummaries };
}
