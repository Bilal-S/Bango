import { describe, it, expect, beforeEach, vi } from 'vitest';

// ── Hoisted state ──────────────────────────────────────────────────
// `vi.mock` is hoisted above all imports, so any variable it closes over
// must also be hoisted (via `vi.hoisted`) to avoid a Temporal Dead Zone
// ReferenceError when the mocked `listen` runs eagerly during module import.
const { eventCallbacks } = vi.hoisted(() => ({
  eventCallbacks: new Map<string, (event: { payload: unknown }) => void | Promise<void>>(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(
    async (event: string, callback: (event: { payload: unknown }) => void | Promise<void>) => {
      eventCallbacks.set(event, callback);
      return () => undefined;
    }
  ),
}));

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Import after mocks are set up (vi.mock is hoisted above imports)
import {
  requestArticleAiSummary,
  parseAiSummary,
  pendingSummaries,
  isUnifiedSummary,
} from '@/composables/use-ai-summary';
import type { AiSummaryData } from '@/composables/use-ai-summary';
import { useToast } from '@/composables/use-toast';
import { shimLocalStorage } from '../helpers/fixtures';

describe('use-ai-summary', () => {
  beforeEach(async () => {
    // happy-dom's localStorage lacks removeItem/clear; install a full shim so
    // the section-summaries toggle read/write/clear works.
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });

    // Wait for the module-level `ensureGlobalListeners()` (fire-and-forget
    // on import) to finish registering both event listeners.
    await vi.waitFor(() => {
      expect(eventCallbacks.has('article-ai-summary-complete')).toBe(true);
      expect(eventCallbacks.has('article-ai-summary-error')).toBe(true);
    });

    // Reset module-level mutable state.
    // NOTE: do NOT clear `eventCallbacks` - listeners are registered once
    // (guarded by `listenersInitialized`) and clearing the map would make
    // them unreachable for the rest of the suite.
    pendingSummaries.value.clear();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue('ok');
    localStorage.removeItem('bango-section-summaries');

    // Clear any lingering toasts from prior tests.
    const { toasts, dismiss } = useToast();
    for (const t of [...toasts.value]) {
      dismiss(t.id);
    }
  });

  // ── requestArticleAiSummary ───────────────────────────────────────

  it('adds to pendingSummaries and shows info toast on request', async () => {
    const { toasts } = useToast();

    await requestArticleAiSummary('a1', 'Test Title');

    expect(pendingSummaries.value.has('a1')).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('generate_article_ai_summary', {
      articleId: 'a1',
      includeSectionSummaries: false,
    });
    expect(toasts.value).toHaveLength(1);
    expect(toasts.value[0]!.message).toBe('Submitted for AI summary');
    expect(toasts.value[0]!.type).toBe('info');
  });

  it('forwards includeSectionSummaries=false by default (toggle absent)', async () => {
    await requestArticleAiSummary('a1', 'Test Title');
    expect(mockInvoke).toHaveBeenCalledWith('generate_article_ai_summary', {
      articleId: 'a1',
      includeSectionSummaries: false,
    });
  });

  it('forwards includeSectionSummaries=true when localStorage toggle is on', async () => {
    localStorage.setItem('bango-section-summaries', 'true');
    await requestArticleAiSummary('a1', 'Test Title');
    expect(mockInvoke).toHaveBeenCalledWith('generate_article_ai_summary', {
      articleId: 'a1',
      includeSectionSummaries: true,
    });
  });

  it('explicit includeSections param overrides the localStorage toggle', async () => {
    localStorage.setItem('bango-section-summaries', 'true');
    await requestArticleAiSummary('a1', 'Test Title', undefined, false);
    expect(mockInvoke).toHaveBeenCalledWith('generate_article_ai_summary', {
      articleId: 'a1',
      includeSectionSummaries: false,
    });
  });

  it('explicit includeSections=true forwards true even when toggle is off', async () => {
    // Toggle absent (default off).
    await requestArticleAiSummary('a1', 'Test Title', undefined, true);
    expect(mockInvoke).toHaveBeenCalledWith('generate_article_ai_summary', {
      articleId: 'a1',
      includeSectionSummaries: true,
    });
  });

  it('invokes onComplete callback and clears pending on complete event', async () => {
    const { toasts } = useToast();
    const onComplete = vi.fn().mockResolvedValue(undefined);

    await requestArticleAiSummary('a1', 'Test Title', onComplete);
    expect(pendingSummaries.value.has('a1')).toBe(true);

    // Simulate the backend emitting the success event.
    const completeCb = eventCallbacks.get('article-ai-summary-complete')!;
    await completeCb({ payload: { articleId: 'a1', title: 'Test Title' } });

    expect(onComplete).toHaveBeenCalledWith('a1');
    expect(pendingSummaries.value.has('a1')).toBe(false);

    const successToast = toasts.value.find((t) => t.message.includes('Summary complete'));
    expect(successToast).toBeTruthy();
    expect(successToast!.type).toBe('success');
  });

  it('cleans up the callback after firing so it does not fire twice', async () => {
    const onComplete = vi.fn().mockResolvedValue(undefined);

    await requestArticleAiSummary('a1', 'Test Title', onComplete);
    const completeCb = eventCallbacks.get('article-ai-summary-complete')!;

    await completeCb({ payload: { articleId: 'a1', title: 'Test Title' } });
    expect(onComplete).toHaveBeenCalledTimes(1);

    // Fire the same event again - callback was deleted, so no double-fire.
    await completeCb({ payload: { articleId: 'a1', title: 'Test Title' } });
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it('clears pendingSummaries and shows error toast on error event', async () => {
    const { toasts } = useToast();

    await requestArticleAiSummary('a2', 'Another Title');
    expect(pendingSummaries.value.has('a2')).toBe(true);

    // Simulate the backend emitting an error event.
    const errorCb = eventCallbacks.get('article-ai-summary-error')!;
    await errorCb({ payload: { articleId: 'a2', error: 'LLM timed out' } });

    expect(pendingSummaries.value.has('a2')).toBe(false);

    const errorToast = toasts.value.find((t) => t.message.includes('AI summary failed'));
    expect(errorToast).toBeTruthy();
    expect(errorToast!.type).toBe('error');
  });

  it('cleans up pendingSummaries when invoke rejects immediately', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('Network error'));
    const { toasts } = useToast();

    await requestArticleAiSummary('a3', 'Failed Title');

    // The `.catch()` on the invoke promise runs asynchronously.
    await vi.waitFor(() => {
      expect(pendingSummaries.value.has('a3')).toBe(false);
    });

    const errorToast = toasts.value.find((t) => t.message.includes('AI summary failed'));
    expect(errorToast).toBeTruthy();
    expect(errorToast!.message).toContain('Network error');
  });

  // ── parseAiSummary ────────────────────────────────────────────────

  describe('parseAiSummary', () => {
    it('returns null for null, undefined, or empty string', () => {
      expect(parseAiSummary(null)).toBeNull();
      expect(parseAiSummary(undefined)).toBeNull();
      expect(parseAiSummary('')).toBeNull();
    });

    it('returns null for invalid JSON', () => {
      expect(parseAiSummary('not json')).toBeNull();
      expect(parseAiSummary('{ broken')).toBeNull();
    });

    it('returns null for valid JSON missing summary_150_250_words', () => {
      expect(parseAiSummary('{"foo": "bar"}')).toBeNull();
    });

    it('parses a well-formed AI summary JSON string', () => {
      const valid = {
        field: 'public_health',
        subfield: 'nutrition',
        structured_extraction: { objective: 'Test objective' },
        summary_150_250_words: 'This is a summary.',
        key_insights: ['insight 1', 'insight 2'],
        keywords: ['kw1', 'kw2'],
      };

      const result = parseAiSummary(JSON.stringify(valid));

      expect(result).not.toBeNull();
      expect(result!.field).toBe('public_health');
      expect(result!.subfield).toBe('nutrition');
      expect(result!.summary_150_250_words).toBe('This is a summary.');
      expect(result!.key_insights).toEqual(['insight 1', 'insight 2']);
      expect(result!.keywords).toEqual(['kw1', 'kw2']);
      // Backward-compat: v1 blob has no section_summaries.
      expect(result!.section_summaries).toBeUndefined();
    });

    it('parses a v2 blob with section_summaries', () => {
      const v2 = {
        schema_version: 2,
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'Whole-paper summary.',
        key_insights: [],
        keywords: [],
        section_summaries: [
          {
            section: 'Methods',
            summary: 'We did an RCT.',
            key_points: ['N=1000'],
            study_design: 'Randomized Controlled Trial',
          },
          {
            section: 'Results',
            summary: 'BMI fell.',
            key_points: ['d=0.2'],
            effect_size: 'd=0.2',
            confidence_interval: '95% CI [0.1, 0.3]',
          },
          {
            section: 'Discussion',
            summary: 'Policy relevant.',
            key_points: [],
          },
        ],
      };

      const result = parseAiSummary(JSON.stringify(v2));

      expect(result).not.toBeNull();
      expect(result!.schema_version).toBe(2);
      expect(result!.section_summaries).toBeDefined();
      expect(result!.section_summaries).toHaveLength(3);
      expect(result!.section_summaries![0]!.section).toBe('Methods');
      expect(result!.section_summaries![0]!.study_design).toBe('Randomized Controlled Trial');
      expect(result!.section_summaries![1]!.section).toBe('Results');
      expect(result!.section_summaries![1]!.effect_size).toBe('d=0.2');
      expect(result!.section_summaries![1]!.confidence_interval).toBe('95% CI [0.1, 0.3]');
      expect(result!.section_summaries![2]!.section).toBe('Discussion');
    });

    it('parses a v2 blob with empty section_summaries array', () => {
      const v2Empty = {
        schema_version: 2,
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'Whole-paper summary.',
        key_insights: [],
        keywords: [],
        section_summaries: [],
      };

      const result = parseAiSummary(JSON.stringify(v2Empty));

      expect(result).not.toBeNull();
      expect(result!.section_summaries).toBeDefined();
      expect(result!.section_summaries).toHaveLength(0);
    });

    // ── Tier 4.3: parseAiSummary preserves table markdown column ────────

    it('parseAiSummary_preserves_table_markdown_field', () => {
      const v2WithTableMarkdown = {
        schema_version: 2,
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'Whole-paper summary.',
        key_insights: [],
        keywords: [],
        tables: [
          {
            number: '1',
            caption: 'Study characteristics.',
            markdown: '| Col1 | Col2 |\n| --- | --- |\n| a | b |',
            description: 'Describes the sample.',
          },
        ],
      };

      const result = parseAiSummary(JSON.stringify(v2WithTableMarkdown));

      expect(result).not.toBeNull();
      expect(result!.tables).toBeDefined();
      expect(result!.tables).toHaveLength(1);
      expect(result!.tables![0]!.markdown).toBe('| Col1 | Col2 |\n| --- | --- |\n| a | b |');
    });
  });

  // ── Tier 4.3: isUnifiedSummary ──────────────────────────────────────────

  describe('isUnifiedSummary', () => {
    it('isUnifiedSummary_true_for_v2_blob', () => {
      const data: AiSummaryData = {
        schema_version: 2,
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'A summary.',
        key_insights: [],
        keywords: [],
      };
      expect(isUnifiedSummary(data)).toBe(true);
    });

    it('isUnifiedSummary_true_for_v3_blob (forward-compatible)', () => {
      const data: AiSummaryData = {
        schema_version: 3,
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'A summary.',
        key_insights: [],
        keywords: [],
      };
      expect(isUnifiedSummary(data)).toBe(true);
    });

    it('isUnifiedSummary_false_for_v1_blob', () => {
      const data: AiSummaryData = {
        schema_version: 1,
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'A summary.',
        key_insights: [],
        keywords: [],
      };
      expect(isUnifiedSummary(data)).toBe(false);
    });

    it('isUnifiedSummary_false_for_absent_schema_version', () => {
      const data: AiSummaryData = {
        field: 'medicine',
        subfield: 'public_health',
        structured_extraction: {},
        summary_150_250_words: 'A summary.',
        key_insights: [],
        keywords: [],
      };
      expect(isUnifiedSummary(data)).toBe(false);
    });

    it('isUnifiedSummary_false_for_null', () => {
      expect(isUnifiedSummary(null)).toBe(false);
    });
  });
});
