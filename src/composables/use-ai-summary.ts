import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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

// ── Module-level singleton event listeners ─────────────────────────
// These persist regardless of component lifecycle so that toasts and
// pending-state cleanup work even when the user navigates away.

type SummaryCallback = (articleId: string) => Promise<void>;
const summaryCallbacks = new Map<string, SummaryCallback>();
const errorCallbacks = new Map<string, SummaryCallback>();

let listenersInitialized = false;

/** Lazily register the global Tauri event listeners (once only). */
async function ensureGlobalListeners(): Promise<void> {
  if (listenersInitialized) return;
  listenersInitialized = true;

  await listen<{ articleId: string; title: string }>(
    'article-ai-summary-complete',
    async (event) => {
      pendingSummaries.value.delete(event.payload.articleId);
      const { show } = useToast();
      show(`Summary complete for: ${event.payload.title}`, 'success');

      // Invoke any registered callback for this article
      const cb = summaryCallbacks.get(event.payload.articleId);
      if (cb) {
        try {
          await cb(event.payload.articleId);
        } catch (e) {
          console.error('AI summary callback error:', e);
        } finally {
          summaryCallbacks.delete(event.payload.articleId);
        }
      }
    }
  );

  await listen<{ articleId: string; error: string }>('article-ai-summary-error', async (event) => {
    pendingSummaries.value.delete(event.payload.articleId);
    const { show } = useToast();
    show(`AI summary failed: ${event.payload.error}`, 'error');

    const cb = errorCallbacks.get(event.payload.articleId);
    if (cb) {
      try {
        await cb(event.payload.articleId);
      } catch (e) {
        console.error('AI summary error callback error:', e);
      } finally {
        errorCallbacks.delete(event.payload.articleId);
      }
    }
  });
}

// Eagerly initialize listeners when module is imported
void ensureGlobalListeners();

// ── Public API ──────────────────────────────────────────────────────

/**
 * Request an AI summary for an article's full text.
 * Shows a toast immediately and processes asynchronously.
 * @param onComplete - Optional callback invoked when summary completes (even
 *   across navigation). Automatically cleaned up after firing.
 */
export async function requestArticleAiSummary(
  articleId: string,
  articleTitle: string,
  onComplete?: (articleId: string) => Promise<void>
) {
  const { show } = useToast();
  try {
    show('Submitted for AI summary', 'info');
    pendingSummaries.value.add(articleId);

    // Register the completion callback if provided
    if (onComplete) {
      summaryCallbacks.set(articleId, onComplete);
    }

    // Fire-and-forget: the command is async and emits an event on success.
    invoke<string>('generate_article_ai_summary', { articleId }).catch((e: unknown) => {
      pendingSummaries.value.delete(articleId);
      summaryCallbacks.delete(articleId);
      const msg = e instanceof Error ? e.message : String(e);
      show(`AI summary failed: ${msg}`, 'error');
    });
  } catch (e) {
    pendingSummaries.value.delete(articleId);
    summaryCallbacks.delete(articleId);
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
