import { computed, ref } from 'vue';
import { useArticlesStore } from '@/stores/articles';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { stripUuidFromDetails } from '@/utils/formatters';
import type { WikiStatus } from '@/types/wiki';
interface StatusCounts {
  total: number;
  duplicate: number;
  working: number;
  included: number;
  rejected: number;
}

interface ScreeningProgress {
  screened: number;
  total: number;
  percentage: number;
}

/** A single audit entry or a group of import entries */
interface GroupedAuditEntry {
  id: string;
  action: string;
  source: string;
  timestamp: string;
  details: string | null;
  /** First 40 chars of article title for context */
  articleTitle?: string | null;
  /** Article ID for navigation (null for system/import-grouped entries) */
  articleId?: string | null;
  /** For grouped imports: how many articles were imported */
  count?: number;
}

/** Module-level singleton - true once the first dashboard data load completes */
export const initialDataLoaded = ref(false);

/**
 * Dashboard CTA button state. Priority order:
 *   1. `connect_llm`    - LLM not configured
 *   2. `start_screening` - LLM ok AND working articles await screening
 *   3. `build_wiki`     - LLM ok, screening done, wiki not yet built
 *   4. `review_wiki`    - LLM ok, screening done, wiki exists
 */
type DashboardCtaState = 'connect_llm' | 'start_screening' | 'build_wiki' | 'review_wiki';

interface DashboardCta {
  /** Material Symbols icon name (reused from the existing icon set). */
  icon: string;
  /** Button label. */
  label: string;
  /** Route to navigate to on click. */
  route: string;
  /** The resolved state (for tests / debugging). */
  state: DashboardCtaState;
}

export function useDashboard() {
  const articlesStore = useArticlesStore();
  const auditStore = useAuditStore();
  const screeningStore = useScreeningStore();

  // ── CTA signals (fetched in refresh(), non-fatal) ───────────────────────
  /**
   * Reactive "is the LLM configured?" gate sourced from the canonical Pinia
   * store. Replaces the one-shot `has_llm_config` IPC probe.
   */
  const llmConfigured = useLlmConfigured();
  /**
   * True when the wiki is initialized AND has at least one generated page
   * (mirrors the `chat-view.vue` wikiReady test: `initialized && pageCount > 0`).
   */
  const wikiBuilt = ref(false);

  const counts = computed<StatusCounts>(() => {
    const all = articlesStore.articles;
    return {
      total: all.length,
      duplicate: all.filter((a) => a.status === 'duplicate').length,
      working: all.filter((a) => a.status === 'working').length,
      included: all.filter((a) => a.status === 'included').length,
      rejected: all.filter((a) => a.status === 'rejected').length,
    };
  });

  const screeningProgress = computed<ScreeningProgress>(() => {
    // Use live engine progress when a screening run is active
    const live = screeningStore.progress;
    if (live && live.isRunning) {
      return {
        screened: live.completed,
        total: live.total,
        percentage: screeningStore.percentage,
      };
    }
    // Fall back to article-count snapshot
    const total = articlesStore.articles.length;
    const screened = articlesStore.articles.filter((a) => a.aiDecision !== null).length;
    const percentage = total > 0 ? Math.round((screened / total) * 100) : 0;
    return { screened, total, percentage };
  });

  /** Non-duplicate article count (Total Articles in summary) */
  const totalNonDuplicate = computed(
    () =>
      articlesStore.articles.length -
      articlesStore.articles.filter((a) => a.status === 'duplicate').length
  );

  /** Articles screened by AI (any non-duplicate with an AI decision, including overridden ones) */
  const screenedByAi = computed(
    () =>
      articlesStore.articles.filter((a) => a.aiDecision !== null && a.status !== 'duplicate').length
  );

  /** Articles screened by user (manually included/rejected without any AI decision) */
  const screenedByUser = computed(
    () =>
      articlesStore.articles.filter(
        (a) => (a.status === 'included' || a.status === 'rejected') && a.aiDecision === null
      ).length
  );

  /** Screening progress percentage: (screenedByAi + screenedByUser) / totalNonDuplicate */
  const screeningPercentage = computed(() => {
    const total = totalNonDuplicate.value;
    if (total === 0) return 0;
    const screened = screenedByAi.value + screenedByUser.value;
    return Math.round((screened / total) * 100);
  });

  const hasArticles = computed(() => articlesStore.articles.length > 0);

  /**
   * Resolved CTA state (pure computed over `llmConfigured`, `counts.working`, `wikiBuilt`).
   */
  const ctaState = computed<DashboardCtaState>(() => {
    if (!llmConfigured.value) return 'connect_llm';
    if (counts.value.working > 0) return 'start_screening';
    if (!wikiBuilt.value) return 'build_wiki';
    return 'review_wiki';
  });

  /** CTA button manifest (icon + label + route) by state. */
  const CTA_BY_STATE: Record<DashboardCtaState, DashboardCta> = {
    connect_llm: { icon: 'link', label: 'Connect LLM', route: '/settings', state: 'connect_llm' },
    start_screening: {
      icon: 'play_arrow',
      label: 'Start AI Screening',
      route: '/screening',
      state: 'start_screening',
    },
    build_wiki: {
      icon: 'local_library',
      label: 'Build Wiki',
      route: '/wiki',
      state: 'build_wiki',
    },
    review_wiki: {
      icon: 'local_library',
      label: 'Review Wiki',
      route: '/wiki',
      state: 'review_wiki',
    },
  };
  const cta = computed<DashboardCta>(() => CTA_BY_STATE[ctaState.value]);

  const loading = computed(() => articlesStore.loading || auditStore.loading);
  const loadingMoreActivities = computed(() => auditStore.loadingMore);
  const hasMoreActivities = computed(() => auditStore.hasMore);
  const error = computed(() => articlesStore.error);

  /** Activity feed - client-side sort as defense-in-depth ensuring newest-first. */
  const groupedAudit = computed<GroupedAuditEntry[]>(() => {
    const items: GroupedAuditEntry[] = auditStore.feed.map((entry) => ({
      id: entry.id,
      action: entry.kind === 'import' ? 'import' : (entry.action ?? ''),
      source: entry.kind === 'import' ? 'system' : (entry.source ?? ''),
      timestamp: entry.timestamp,
      details: stripUuidFromDetails(entry.filename ?? entry.details),
      articleTitle: entry.articleTitle ?? null,
      articleId: entry.articleId ?? null,
      count: entry.count ?? undefined,
    }));
    items.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
    return items;
  });

  /** Load more activity entries (pagination) */
  async function loadMoreActivities(): Promise<void> {
    await auditStore.loadMore();
  }

  /**
   * Full refresh of articles + audit + wiki CTA signals from the DB.
   * The LLM-configured signal is reactive (Pinia store), not probed here.
   */
  async function refresh(): Promise<void> {
    articlesStore.invalidate();
    auditStore.invalidate();
    // Fire the three remaining loads in parallel; the wiki probe swallows
    // errors so a backend hiccup doesn't block the dashboard render.
    const wikiOk = await probeWikiBuilt();
    await Promise.all([articlesStore.fetchIfNeeded(), auditStore.fetchIfNeeded()]);
    wikiBuilt.value = wikiOk;
    initialDataLoaded.value = true;
  }

  /** Non-fatal wiki readiness probe. Returns true when initialized + has pages. */
  async function probeWikiBuilt(): Promise<boolean> {
    try {
      const status = await tauriCommand<WikiStatus>('wiki_get_status');
      return !!status.initialized && status.pageCount > 0;
    } catch {
      return false;
    }
  }

  return {
    counts,
    screeningProgress,
    totalNonDuplicate,
    screenedByAi,
    screenedByUser,
    groupedAudit,
    loading,
    loadingMoreActivities,
    hasMoreActivities,
    error,
    hasArticles,
    screeningPercentage,
    // Dashboard CTA (dynamic primary button)
    cta,
    ctaState,
    llmConfigured,
    wikiBuilt,
    refresh,
    loadMoreActivities,
  };
}

export function formatAuditAction(action: string): string {
  const LABELS: Record<string, string> = {
    import: 'Import',
    dedup_merge: 'Duplicate Merged',
    dedup_flag: 'Duplicate Flagged',
    dedup_auto: 'Auto Dedup',
    status_change: 'Status Change',
    note_add: 'Note Added',
    tag_add: 'Tag Added',
    tag_remove: 'Tag Removed',
    label_add: 'Label Added',
    label_remove: 'Label Removed',
    criteria_match: 'Criteria Matched',
    ai_screen: 'AI Screening',
    ai_screen_enhanced: 'AI Enhanced Screening',
    manual_override: 'Manual Override',
    ai_summary: 'AI Summary',
    reference_import: 'Reference Imported',
    reference_match: 'Reference Matched',
    translation: 'Translation',
    translation_error: 'Translation Failed',
    figure_descriptions: 'Figure Descriptions',
    wiki_ingest_error: 'Wiki Ingest Error',
    search_strategy: 'Search Strategy',
    metadata_edit: 'Metadata Edited',
    error: 'Error',
  };
  return LABELS[action] ?? action;
}

export function formatRelativeTime(isoTimestamp: string): string {
  const parts = formatRelativeTimeParts(isoTimestamp);
  return `${parts.value} ${parts.suffix}`;
}

/**
 * Split a relative time string into a value (e.g. "36m") and a suffix (e.g.
 * "ago") so the dashboard can stack them in a compact right-aligned column.
 * Returns `{ value: "just", suffix: "now" }` for recent timestamps.
 */
export function formatRelativeTimeParts(isoTimestamp: string): {
  value: string;
  suffix: string;
} {
  const now = Date.now();
  const then = new Date(isoTimestamp).getTime();
  const diffMs = now - then;
  const diffSeconds = Math.floor(diffMs / 1000);
  const diffMinutes = Math.floor(diffSeconds / 60);
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffSeconds < 60) return { value: 'just', suffix: 'now' };
  if (diffMinutes < 60) return { value: `${diffMinutes}m`, suffix: 'ago' };
  if (diffHours < 24) return { value: `${diffHours}h`, suffix: 'ago' };
  if (diffDays < 7) return { value: `${diffDays}d`, suffix: 'ago' };
  return { value: new Date(isoTimestamp).toLocaleDateString(), suffix: '' };
}
