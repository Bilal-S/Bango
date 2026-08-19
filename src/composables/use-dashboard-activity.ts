import { computed } from 'vue';
import { useAuditStore } from '@/stores/audit';
import { stripUuidFromDetails } from '@/utils/formatters';

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

/**
 * Dashboard activity feed (refactor1 T4.3): flattens the audit store's merged
 * feed into display rows (newest-first) and owns the load-more pagination.
 * `useDashboard` re-exposes everything unchanged.
 */
export function useDashboardActivity() {
  const auditStore = useAuditStore();

  const loadingMoreActivities = computed(() => auditStore.loadingMore);
  const hasMoreActivities = computed(() => auditStore.hasMore);

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

  return { groupedAudit, loadingMoreActivities, hasMoreActivities, loadMoreActivities };
}
