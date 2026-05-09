import { computed, ref } from 'vue';
import { useArticlesStore } from '@/stores/articles';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { useCriteriaStore } from '@/stores/criteria';
import { useLlmConfigStore } from '@/stores/llm-config';
import type { ArticleStatus } from '@/types';

export interface StatusCounts {
  total: number;
  duplicate: number;
  working: number;
  included: number;
  rejected: number;
}

export interface ScreeningProgress {
  screened: number;
  total: number;
  percentage: number;
}

/** A single audit entry or a group of import entries */
export interface GroupedAuditEntry {
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

/** Module-level singleton — true once the first dashboard data load completes */
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

  const loading = computed(() => articlesStore.loading || auditStore.loading);
  const error = computed(() => articlesStore.error);

  /** Merged timeline: import activities + other audit entries */
  const groupedAudit = computed<GroupedAuditEntry[]>(() => {
    const nonImport: GroupedAuditEntry[] = auditStore.recentAudit.map((entry) => ({
      id: entry.id,
      action: entry.action,
      source: entry.source,
      timestamp: entry.timestamp,
      details: entry.details,
      articleTitle: entry.articleTitle,
    }));

    const merged: GroupedAuditEntry[] = [
      ...auditStore.importActivities.map(
        (act): GroupedAuditEntry => ({
          id: act.id,
          action: 'import',
          source: 'system',
          timestamp: act.timestamp,
          details: act.filename,
          count: act.count,
        })
      ),
      ...nonImport,
    ];

    merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
    return merged;
  });

  /** Force a full refresh of all stores from the DB */
  async function refresh(): Promise<void> {
    const tagsStore = useTagsStore();
    const labelsStore = useLabelsStore();
    const criteriaStore = useCriteriaStore();
    const llmConfigStore = useLlmConfigStore();

    // Invalidate all stores so they re-fetch
    articlesStore.invalidate();
    auditStore.invalidate();
    screeningStore.invalidate();
    tagsStore.invalidate();
    labelsStore.invalidate();
    criteriaStore.invalidate();
    llmConfigStore.invalidate();

    await Promise.all([
      articlesStore.fetchIfNeeded(),
      auditStore.fetchIfNeeded(),
      screeningStore.fetchIfNeeded(),
      tagsStore.fetchIfNeeded(),
      labelsStore.fetchIfNeeded(),
      criteriaStore.fetchIfNeeded(),
      llmConfigStore.fetchIfNeeded(),
    ]);
    initialDataLoaded.value = true;
  }

  return {
    counts,
    screeningProgress,
    totalNonDuplicate,
    screenedByAi,
    screenedByUser,
    groupedAudit,
    loading,
    error,
    hasArticles,
    screeningPercentage,
    refresh,
  };
}

export function formatStatusLabel(status: ArticleStatus): string {
  const LABELS: Record<ArticleStatus, string> = {
    duplicate: 'Duplicates',
    working: 'Working',
    included: 'Included',
    rejected: 'Rejected',
  };
  return LABELS[status] ?? status;
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
