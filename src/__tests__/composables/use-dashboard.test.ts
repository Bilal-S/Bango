import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { formatAuditAction, formatRelativeTimeParts } from '@/composables/use-dashboard';

describe('formatAuditAction', () => {
  it('returns human-friendly labels for known actions', () => {
    expect(formatAuditAction('import')).toBe('Import');
    expect(formatAuditAction('status_change')).toBe('Status Change');
    expect(formatAuditAction('note_add')).toBe('Note Added');
    expect(formatAuditAction('tag_add')).toBe('Tag Added');
    expect(formatAuditAction('ai_screen')).toBe('AI Screening');
    expect(formatAuditAction('manual_override')).toBe('Manual Override');
    expect(formatAuditAction('translation')).toBe('Translation');
    expect(formatAuditAction('translation_error')).toBe('Translation Failed');
  });

  it('returns the raw action string for unknown actions', () => {
    expect(formatAuditAction('unknown_action')).toBe('unknown_action');
  });

  it('covers all actions in the AuditAction type', () => {
    // Every action in the AuditAction union must have a label so the
    // dashboard never leaks raw snake_case to the user.
    const allActions = [
      'import',
      'dedup_merge',
      'dedup_flag',
      'status_change',
      'note_add',
      'tag_add',
      'tag_remove',
      'label_add',
      'label_remove',
      'criteria_match',
      'ai_screen',
      'manual_override',
      'ai_summary',
      'reference_import',
      'reference_match',
      'error',
      'translation',
      'translation_error',
    ];
    for (const action of allActions) {
      const label = formatAuditAction(action);
      // The label must differ from the raw snake_case action (i.e. it was
      // found in the LABELS map and converted to a human-friendly string).
      expect(label).not.toBe(action);
    }
  });
});

describe('formatRelativeTimeParts', () => {
  it('returns "just" / "now" for timestamps less than 60 seconds ago', () => {
    const ts = new Date(Date.now() - 30_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('just');
    expect(parts.suffix).toBe('now');
  });

  it('returns minutes with "ago" suffix', () => {
    const ts = new Date(Date.now() - 36 * 60_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('36m');
    expect(parts.suffix).toBe('ago');
  });

  it('returns hours with "ago" suffix', () => {
    const ts = new Date(Date.now() - 2 * 3_600_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('2h');
    expect(parts.suffix).toBe('ago');
  });

  it('returns days with "ago" suffix', () => {
    const ts = new Date(Date.now() - 3 * 86_400_000).toISOString();
    const parts = formatRelativeTimeParts(ts);
    expect(parts.value).toBe('3d');
    expect(parts.suffix).toBe('ago');
  });
});

// ── useDashboard composable tests ─────────────────────────────────

import { ref, computed } from 'vue';
import type { Ref } from 'vue';

interface MockAuditFeedEntry {
  id: string;
  action: string | null;
  source: string | null;
  timestamp: string;
  details?: string | null;
  filename?: string | null;
  articleTitle?: string | null;
  articleId?: string | null;
  count?: number;
  kind?: string;
}

interface MockScreeningProgress {
  total: number;
  completed: number;
  included: number;
  rejected: number;
  errors: number;
  isRunning: boolean;
  currentArticleTitles: string[];
  elapsedMs: number;
  estimatedRemainingMs: number | null;
}

const mockArticles: Ref<unknown[]> = ref([]);
const mockAuditFeed: Ref<MockAuditFeedEntry[]> = ref([]);
const mockScreeningProgress: Ref<MockScreeningProgress | null> = ref(null);

vi.mock('@/stores/articles', () => ({
  useArticlesStore: () => ({
    get articles() {
      return mockArticles.value;
    },
    loading: ref(false),
    error: ref(null),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
    invalidate: vi.fn(),
  }),
}));

vi.mock('@/stores/audit', () => ({
  useAuditStore: () => ({
    get feed() {
      return mockAuditFeed.value;
    },
    loading: ref(false),
    error: ref(null),
    loadingMore: ref(false),
    hasMore: ref(false),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
    invalidate: vi.fn(),
    loadMore: vi.fn().mockResolvedValue(undefined),
  }),
}));

vi.mock('@/stores/screening', () => ({
  useScreeningStore: () => {
    const progressRef = computed(() => mockScreeningProgress.value);
    return {
      get progress() {
        return progressRef.value;
      },
      error: ref(null),
      initialized: ref(true),
      get percentage() {
        const p = progressRef.value;
        if (!p) return 0;
        return p.total > 0 ? Math.round((p.completed / p.total) * 100) : 0;
      },
    };
  },
}));

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

// Mock the LLM-configured composable so tests can drive `llmConfigured`
// reactively without going through the real Pinia store. The dashboard no
// longer probes `has_llm_config` itself; the gate is owned by the store.
const mockLlmConfigured = ref(false);
vi.mock('@/composables/use-llm-configured', () => ({
  useLlmConfigured: () => mockLlmConfigured,
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useDashboard } from '@/composables/use-dashboard';
import { makeArticle } from '../helpers/fixtures';

function makeAuditEntry(
  overrides: Partial<{
    id: string;
    action: string;
    source: string;
    timestamp: string;
    details: string | null;
    articleTitle: string | null;
    articleId: string | null;
    count: number;
    kind: string;
    filename: string | null;
  }> = {}
) {
  return {
    id: overrides.id ?? 'ae1',
    action: overrides.action ?? 'import',
    source: overrides.source ?? 'system',
    timestamp: overrides.timestamp ?? '2025-01-15T10:00:00Z',
    details: overrides.details ?? null,
    filename: overrides.filename ?? null,
    articleTitle: overrides.articleTitle ?? null,
    articleId: overrides.articleId ?? null,
    count: overrides.count ?? undefined,
    kind: overrides.kind ?? undefined,
  };
}

describe('useDashboard', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    mockArticles.value = [];
    mockAuditFeed.value = [];
    mockScreeningProgress.value = null;
    mockLlmConfigured.value = false;
  });

  describe('counts', () => {
    it('computes status counts from articles', () => {
      mockArticles.value = [
        makeArticle({ id: 'a1', status: 'included' }),
        makeArticle({ id: 'a2', status: 'working' }),
        makeArticle({ id: 'a3', status: 'rejected' }),
        makeArticle({ id: 'a4', status: 'duplicate' }),
        makeArticle({ id: 'a5', status: 'included' }),
      ];

      const { counts } = useDashboard();
      expect(counts.value.total).toBe(5);
      expect(counts.value.working).toBe(1);
      expect(counts.value.included).toBe(2);
      expect(counts.value.rejected).toBe(1);
      expect(counts.value.duplicate).toBe(1);
    });

    it('handles empty article list', () => {
      const { counts } = useDashboard();
      expect(counts.value.total).toBe(0);
    });
  });

  describe('screeningProgress', () => {
    it('uses live engine progress when running', () => {
      mockScreeningProgress.value = {
        total: 100,
        completed: 42,
        included: 20,
        rejected: 22,
        errors: 0,
        isRunning: true,
        currentArticleTitles: [],
        elapsedMs: 5000,
        estimatedRemainingMs: 7000,
      };

      const { screeningProgress } = useDashboard();
      expect(screeningProgress.value.screened).toBe(42);
      expect(screeningProgress.value.total).toBe(100);
    });

    it('falls back to article-count snapshot when not running', () => {
      mockArticles.value = [
        makeArticle({ id: 'a1', aiDecision: 'include' }),
        makeArticle({ id: 'a2', aiDecision: 'exclude' }),
        makeArticle({ id: 'a3', aiDecision: null }),
        makeArticle({ id: 'a4', aiDecision: 'include' }),
      ];

      const { screeningProgress } = useDashboard();
      expect(screeningProgress.value.screened).toBe(3);
      expect(screeningProgress.value.total).toBe(4);
    });
  });

  describe('screened counts', () => {
    it('separates AI-screened from user-screened', () => {
      mockArticles.value = [
        makeArticle({ id: 'a1', status: 'included', aiDecision: 'include' }),
        makeArticle({ id: 'a2', status: 'rejected', aiDecision: 'exclude' }),
        makeArticle({ id: 'a3', status: 'included', aiDecision: null }),
        makeArticle({ id: 'a4', status: 'working', aiDecision: null }),
        makeArticle({ id: 'a5', status: 'duplicate', aiDecision: 'include' }),
      ];

      const { screenedByAi, screenedByUser, totalNonDuplicate, screeningPercentage } =
        useDashboard();

      expect(screenedByAi.value).toBe(2);
      expect(screenedByUser.value).toBe(1);
      expect(totalNonDuplicate.value).toBe(4);
      expect(screeningPercentage.value).toBe(75);
    });
  });

  describe('CTA state machine', () => {
    it('returns connect_llm when LLM is not configured', () => {
      // `mockLlmConfigured` defaults to false (set in beforeEach); no
      // `has_llm_config` IPC mock is needed because the dashboard reads the
      // gate from the store composable now.
      const { cta, ctaState } = useDashboard();

      expect(ctaState.value).toBe('connect_llm');
      expect(cta.value.icon).toBe('link');
      expect(cta.value.route).toBe('/settings');
    });

    it('returns start_screening when LLM configured and working articles exist', () => {
      mockArticles.value = [makeArticle({ id: 'a1', status: 'working' })];
      mockLlmConfigured.value = true;

      const { cta, ctaState } = useDashboard();

      expect(ctaState.value).toBe('start_screening');
      expect(cta.value.route).toBe('/screening');
    });

    it('returns build_wiki when LLM configured, no working, wiki not built', () => {
      mockArticles.value = [makeArticle({ id: 'a1', status: 'included' })];
      mockLlmConfigured.value = true;

      const { cta, ctaState, wikiBuilt } = useDashboard();
      wikiBuilt.value = false;

      expect(ctaState.value).toBe('build_wiki');
      expect(cta.value.route).toBe('/wiki');
    });

    it('returns review_wiki when LLM configured, no working, wiki built', () => {
      mockArticles.value = [makeArticle({ id: 'a1', status: 'included' })];
      mockLlmConfigured.value = true;

      const { cta, ctaState, wikiBuilt } = useDashboard();
      wikiBuilt.value = true;

      expect(ctaState.value).toBe('review_wiki');
      expect(cta.value.route).toBe('/wiki');
    });
  });

  describe('groupedAudit', () => {
    it('maps feed entries with article context', () => {
      mockAuditFeed.value = [
        makeAuditEntry({
          id: 'ae1',
          action: 'ai_screen',
          source: 'ai',
          timestamp: '2025-01-15T10:00:00Z',
          details: 'Screened as include',
          articleTitle: 'Test Paper',
          articleId: 'b6a3f2e1',
        }),
      ];

      const { groupedAudit } = useDashboard();
      expect(groupedAudit.value).toHaveLength(1);
      const entry = groupedAudit.value[0]!;
      expect(entry.action).toBe('ai_screen');
      expect(entry.source).toBe('ai');
      expect(entry.articleTitle).toBe('Test Paper');
      expect(entry.articleId).toBe('b6a3f2e1');
    });

    it('strips UUIDs from details', () => {
      mockAuditFeed.value = [
        makeAuditEntry({
          details: 'Article b6a3f2e1-1234-5678-9abc-def012345678 added',
        }),
      ];

      const { groupedAudit } = useDashboard();
      expect(groupedAudit.value[0]!.details).toBe('Article added');
    });
  });

  describe('refresh', () => {
    it('calls probes and sets signals', async () => {
      const tauriCmd = vi.mocked(tauriCommand);
      tauriCmd.mockImplementation((cmd: string) => {
        if (cmd === 'wiki_get_status') return Promise.resolve({ initialized: true, pageCount: 5 });
        return Promise.resolve(null);
      });
      mockLlmConfigured.value = true;

      const { refresh, llmConfigured, wikiBuilt } = useDashboard();
      await refresh();

      expect(llmConfigured.value).toBe(true);
      expect(wikiBuilt.value).toBe(true);
    });

    it('defaults to false when probes fail', async () => {
      const tauriCmd = vi.mocked(tauriCommand);
      tauriCmd.mockRejectedValue(new Error('fail'));

      const { refresh, wikiBuilt } = useDashboard();
      await refresh();

      // llmConfigured is driven by the store mock, not the failed probe; the
      // wiki probe failure sets wikiBuilt to false.
      expect(wikiBuilt.value).toBe(false);
    });

    it('wikiBuilt is false when pageCount is 0', async () => {
      const tauriCmd = vi.mocked(tauriCommand);
      tauriCmd.mockImplementation((cmd: string) => {
        if (cmd === 'wiki_get_status') return Promise.resolve({ initialized: true, pageCount: 0 });
        return Promise.resolve(null);
      });

      const { refresh, wikiBuilt } = useDashboard();
      await refresh();

      expect(wikiBuilt.value).toBe(false);
    });
  });
});
