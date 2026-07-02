import { computed, ref } from 'vue';
import { useArticlesStore } from '@/stores/articles';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';
import { tauriCommand } from '@/composables/use-tauri-command';
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
  /** For grouped imports: how many articles were imported */
  count?: number;
}

/** Module-level singleton - true once the first dashboard data load completes */
export const initialDataLoaded = ref(false);

/**
 * Dashboard CTA button state. The single primary button on the dashboard
 * adapts to the research workflow stage, in priority order:
 *   1. `connect_llm`    - LLM not configured yet (gate for everything else)
 *   2. `start_screening` - LLM ok AND working articles awaiting screening
 *   3. `build_wiki`     - LLM ok, screening done, wiki not yet built
 *   4. `review_wiki`    - LLM ok, screening done, wiki exists
 */
export type DashboardCtaState = 'connect_llm' | 'start_screening' | 'build_wiki' | 'review_wiki';

export interface DashboardCta {
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
  /** True when `has_llm_config` returns true (LLM provider + key + model set). */
  const llmConfigured = ref(false);
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
   * Resolved dashboard CTA state (priority order - see DashboardCtaState docs).
   * Pure computed over `llmConfigured`, `counts.working`, and `wikiBuilt`.
   */
  const ctaState = computed<DashboardCtaState>(() => {
    if (!llmConfigured.value) return 'connect_llm';
    if (counts.value.working > 0) return 'start_screening';
    if (!wikiBuilt.value) return 'build_wiki';
    return 'review_wiki';
  });

  /**
   * The CTA button manifest (icon + label + route) for the current state.
   * Lookup map keyed by DashboardCtaState; icons are reused from the existing
   * codebase (no new Material Symbols added).
   *   - `link`           - tag-label-management, reference panels, help-tab-reference
   *   - `play_arrow`     - existing dashboard CTA
   *   - `local_library`  - wiki icon (chat-view, nav-sidebar, help-tab-reference)
   */
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
  const hasMoreActivities = computed(() => auditStore.hasMoreAudit || auditStore.hasMoreImports);
  const error = computed(() => articlesStore.error);

  /** Merged timeline: import activities + other audit entries */
  const groupedAudit = computed<GroupedAuditEntry[]>(() => {
    const nonImport: GroupedAuditEntry[] = auditStore.recentAudit.map((entry) => ({
      id: entry.id,
      action: entry.action,
      source: entry.source,
      timestamp: entry.timestamp,
      details: stripUuidFromDetails(entry.details),
      articleTitle: entry.articleTitle,
    }));

    const merged: GroupedAuditEntry[] = [
      ...auditStore.importActivities.map(
        (act): GroupedAuditEntry => ({
          id: act.id,
          action: 'import',
          source: 'system',
          timestamp: act.timestamp,
          details: stripUuidFromDetails(act.filename),
          count: act.count,
        })
      ),
      ...nonImport,
    ];

    merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
    return merged;
  });

  /** Load more activity entries (pagination) */
  async function loadMoreActivities(): Promise<void> {
    await auditStore.loadMore();
  }

  /**
   * Force a full refresh of articles + audit + dashboard-CTA signals from the DB.
   *
   * The CTA signals (`has_llm_config`, `wiki_get_status`) are fetched in
   * parallel with the article/audit loads. Both are non-fatal: on error they
   * default to `false`, which correctly falls the button back to the safest
   * CTA (`connect_llm` if LLM probe fails, `build_wiki` if wiki probe fails).
   */
  async function refresh(): Promise<void> {
    articlesStore.invalidate();
    auditStore.invalidate();
    // Fire all four loads in parallel; the CTA probes swallow errors so a
    // backend hiccup on one endpoint doesn't block the dashboard render.
    const [, , /* articles */ /* audit */ llmOk, wikiOk] = await Promise.all([
      articlesStore.fetchIfNeeded(),
      auditStore.fetchIfNeeded(),
      probeLlmConfigured(),
      probeWikiBuilt(),
    ]);
    llmConfigured.value = llmOk;
    wikiBuilt.value = wikiOk;
    initialDataLoaded.value = true;
  }

  /** Non-fatal `has_llm_config` probe. Returns false on any error. */
  async function probeLlmConfigured(): Promise<boolean> {
    try {
      return await tauriCommand<boolean>('has_llm_config');
    } catch {
      return false;
    }
  }

  /**
   * Non-fatal `wiki_get_status` probe. Returns true only when the wiki is
   * initialized AND has at least one generated page (the same readiness test
   * `chat-view.vue` uses for the wiki-toggle visibility).
   */
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
    import: 'Imported',
    dedup_merge: 'Merged duplicate',
    dedup_flag: 'Flagged duplicate',
    status_change: 'Changed status',
    tag_add: 'Added tag',
    tag_remove: 'Removed tag',
    label_add: 'Added label',
    label_remove: 'Removed label',
    criteria_match: 'Matched criteria',
    ai_screen: 'AI screened',
    manual_override: 'Manual override',
    ai_summary: 'AI summary generated',
  };
  return LABELS[action] ?? action;
}

export function formatRelativeTime(isoTimestamp: string): string {
  const now = Date.now();
  const then = new Date(isoTimestamp).getTime();
  const diffMs = now - then;
  const diffSeconds = Math.floor(diffMs / 1000);
  const diffMinutes = Math.floor(diffSeconds / 60);
  const diffHours = Math.floor(diffMinutes / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffSeconds < 60) return 'just now';
  if (diffMinutes < 60) return `${diffMinutes}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;
  return new Date(isoTimestamp).toLocaleDateString();
}
