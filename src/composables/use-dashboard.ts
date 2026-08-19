import { computed, ref } from 'vue';
import { useArticlesStore } from '@/stores/articles';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useDashboardCta } from '@/composables/use-dashboard-cta';
import { useDashboardActivity } from '@/composables/use-dashboard-activity';
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

/** Module-level singleton - true once the first dashboard data load completes */
export const initialDataLoaded = ref(false);

export function useDashboard() {
  const articlesStore = useArticlesStore();
  const auditStore = useAuditStore();
  const screeningStore = useScreeningStore();

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

  // ── CTA signals (extracted composable; refresh() writes wikiBuilt) ────
  const { llmConfigured, wikiBuilt, ctaState, cta } = useDashboardCta({
    workingCount: computed(() => counts.value.working),
  });

  // ── Activity feed (extracted composable) ─────────────────────────────
  const { groupedAudit, loadingMoreActivities, hasMoreActivities, loadMoreActivities } =
    useDashboardActivity();

  const loading = computed(() => articlesStore.loading || auditStore.loading);
  const error = computed(() => articlesStore.error);

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
