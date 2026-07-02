import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useToast } from './use-toast';

export interface SectionSummary {
  section: string;
  summary: string;
  key_points?: string[];
  study_design?: string;
  sample_size?: string;
  effect_size?: string;
  confidence_interval?: string;
}

/** Tier 2 Phase 4: one LLM-described figure or table caption. */
export interface FigureDescription {
  /** The figure/table number as a string ("1", "2a"). */
  number: string;
  /** The verbatim extracted caption text. */
  caption?: string;
  /** The grounded LLM summary of what the caption states. */
  description?: string;
}

/** Tier 4.2/4.3: one LLM-described table caption with the optional `markdown`
 * GFM column for tables-as-GFM rendering. Extends FigureDescription with the
 * preserved GFM rows extracted from the full text (T2.2). */
export interface TableDescription {
  /** The table number as a string ("1", "2a"). */
  number: string;
  /** The verbatim extracted caption text. */
  caption?: string;
  /** GFM markdown rows extracted from the full text (T2.2). Optional: old
   * blobs without `markdown` render text-only; new blobs populate it when
   * `detect_markdown_tables` found a table for that caption number. */
  markdown?: string;
  /** The grounded LLM summary of what the caption states. */
  description?: string;
}

export interface AiSummaryData {
  schema_version?: number;
  field: string;
  subfield: string;
  // Values may be a scalar string (e.g., `study_type`, `population`) OR an
  // array of strings (e.g., `key_results`, `methods_models`, `limitations`).
  // The extraction prompt's 5 field-specific templates each emit a mix of both
  // shapes; the renderer must handle `string | string[]` per value.
  structured_extraction: Record<string, string | string[]>;
  summary_150_250_words: string;
  key_insights: string[];
  keywords: string[];
  section_summaries?: SectionSummary[];
  /** Tier 2 Phase 4: LLM-described figure captions. Present only on v2 blobs
   * after `generate_figure_descriptions` has run. */
  figures?: FigureDescription[];
  /** Tier 4.2/4.3: LLM-described table captions. Widened to `TableDescription[]`
   * so the optional `markdown` GFM column is available for tables-as-GFM
   * rendering. Old blobs without `markdown` still render (text-only fallback). */
  tables?: TableDescription[];
}

/**
 * Tier 4.3: True when the blob is the v2 superset (enriched view). False for
 * legacy v1/absent schema_version blobs. Drives view branching in
 * `abstract-summary-view.vue` so a v2 blob with an empty `section_summaries`
 * array still renders the enriched shell.
 */
export function isUnifiedSummary(data: AiSummaryData | null): boolean {
  return (data?.schema_version ?? 0) >= 2;
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
 * @param includeSections - When true, also generate per-section summaries
 *   (Methods/Results/Discussion) in the same LLM call. Reads the
 *   `bango-section-summaries` localStorage toggle when omitted.
 */
export async function requestArticleAiSummary(
  articleId: string,
  articleTitle: string,
  onComplete?: (articleId: string) => Promise<void>,
  includeSections?: boolean
) {
  const { show } = useToast();
  try {
    show('Submitted for AI summary', 'info');
    pendingSummaries.value.add(articleId);

    // Register the completion callback if provided
    if (onComplete) {
      summaryCallbacks.set(articleId, onComplete);
    }

    // Resolve the section-summaries flag: explicit param wins, otherwise read
    // the persisted user preference (default off / absent = false).
    const wantSections =
      includeSections ?? localStorage.getItem('bango-section-summaries') === 'true';

    // Fire-and-forget: the command is async and emits an event on success.
    invoke<string>('generate_article_ai_summary', {
      articleId,
      includeSectionSummaries: wantSections,
    }).catch((e: unknown) => {
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
 * Tier 4.4: Request a unified AI summary for an article's full text. This is
 * the recommended replacement for `requestArticleAiSummary` - it auto-routes
 * to the unified path (section calls + figure call + synthesis call) when
 * sections are detectable, and falls back to the monolithic path otherwise.
 * The user does not need to know which path ran.
 *
 * Reuses the same `article-ai-summary-complete` / `article-ai-summary-error`
 * events and the same `pendingSummaries` set as the legacy command so the
 * existing UI wiring works unchanged.
 */
export async function requestUnifiedSummary(
  articleId: string,
  articleTitle: string,
  onComplete?: (articleId: string) => Promise<void>
) {
  const { show } = useToast();
  try {
    show('Submitted for AI summary', 'info');
    pendingSummaries.value.add(articleId);

    // Register the completion callback if provided (reuses the same callback
    // map as the legacy command - the events are identical).
    if (onComplete) {
      summaryCallbacks.set(articleId, onComplete);
    }

    // Fire-and-forget: the command is async and emits an event on success.
    invoke<string>('generate_unified_summary', { articleId }).catch((e: unknown) => {
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
    if (!parsed || typeof parsed !== 'object') return null;
    // Tier 2 lenient fallback (T4 E2E 2026-07-01): accept any blob that has at
    // least ONE substantive field, not just `summary_150_250_words`. This
    // prevents the all-or-nothing failure where a reasoning model returns
    // `{"schema_version":2}` (no summary text) and the view shows "No AI
    // summary" even though the command ran successfully. With lenient parsing,
    // a partial response still renders with whatever fields the model produced.
    const hasDigest = typeof parsed.summary_150_250_words === 'string';
    const hasField = typeof parsed.field === 'string' && parsed.field.length > 0;
    const hasExtraction =
      parsed.structured_extraction &&
      typeof parsed.structured_extraction === 'object' &&
      Object.keys(parsed.structured_extraction).length > 0;
    if (hasDigest || hasField || hasExtraction) {
      // Ensure required fields have safe defaults so the TS interface is satisfied.
      if (!parsed.field) parsed.field = '';
      if (!parsed.subfield) parsed.subfield = '';
      if (!parsed.summary_150_250_words) parsed.summary_150_250_words = '';
      if (!Array.isArray(parsed.key_insights)) parsed.key_insights = [];
      if (!Array.isArray(parsed.keywords)) parsed.keywords = [];
      if (!parsed.structured_extraction) parsed.structured_extraction = {};
      return parsed as AiSummaryData;
    }
    return null;
  } catch {
    return null;
  }
}

/** Module-level pending set for figure-description requests (T2 Phase 4). */
export const pendingFigureDescriptions = ref<Set<string>>(new Set());

type FigureCallback = (articleId: string) => Promise<void>;
const figureCallbacks = new Map<string, FigureCallback>();
const figureErrorCallbacks = new Map<string, FigureCallback>();
let figureListenersInitialized = false;

/** Lazily register the global Tauri event listeners for figure descriptions. */
async function ensureFigureListeners(): Promise<void> {
  if (figureListenersInitialized) return;
  figureListenersInitialized = true;

  await listen<{ articleId: string; title: string }>(
    'article-figure-descriptions-complete',
    async (event) => {
      pendingFigureDescriptions.value.delete(event.payload.articleId);
      const { show } = useToast();
      show(`Figure/table descriptions complete for: ${event.payload.title}`, 'success');
      const cb = figureCallbacks.get(event.payload.articleId);
      if (cb) {
        try {
          await cb(event.payload.articleId);
        } catch (e) {
          console.error('Figure description callback error:', e);
        } finally {
          figureCallbacks.delete(event.payload.articleId);
        }
      }
    }
  );

  await listen<{ articleId: string; error: string }>(
    'article-figure-descriptions-error',
    async (event) => {
      pendingFigureDescriptions.value.delete(event.payload.articleId);
      const { show } = useToast();
      show(`Figure/table descriptions failed: ${event.payload.error}`, 'error');
      const cb = figureErrorCallbacks.get(event.payload.articleId);
      if (cb) {
        try {
          await cb(event.payload.articleId);
        } catch (e) {
          console.error('Figure description error callback error:', e);
        } finally {
          figureErrorCallbacks.delete(event.payload.articleId);
        }
      }
    }
  );
}

void ensureFigureListeners();

/**
 * Request LLM descriptions for an article's figure/table captions (T2 Phase 4).
 * One batched orchestrator call per article, grounded in the extracted caption
 * text. Shows a toast immediately and processes asynchronously. The result is
 * merged into the existing `full_text_ai_summary` blob under `figures`/`tables`.
 */
export async function requestFigureDescriptions(
  articleId: string,
  articleTitle: string,
  onComplete?: (articleId: string) => Promise<void>
) {
  const { show } = useToast();
  try {
    show('Submitted for figure/table descriptions', 'info');
    pendingFigureDescriptions.value.add(articleId);
    if (onComplete) {
      figureCallbacks.set(articleId, onComplete);
    }
    invoke<string>('generate_figure_descriptions', { articleId }).catch((e: unknown) => {
      pendingFigureDescriptions.value.delete(articleId);
      figureCallbacks.delete(articleId);
      const msg = e instanceof Error ? e.message : String(e);
      show(`Figure/table descriptions failed: ${msg}`, 'error');
    });
  } catch (e) {
    pendingFigureDescriptions.value.delete(articleId);
    figureCallbacks.delete(articleId);
    const msg = e instanceof Error ? e.message : String(e);
    show(`Figure/table descriptions failed: ${msg}`, 'error');
  }
  void articleTitle;
}
