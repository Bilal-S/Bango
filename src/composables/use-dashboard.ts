import { ref, computed, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { Article, AuditEntry, ArticleStatus } from '@/types';

export interface StatusCounts {
  total: number;
  imported: number;
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
  /** For grouped imports: how many articles were imported */
  count?: number;
}

/** Shape returned by the Rust `get_import_activities` command */
interface ImportActivity {
  id: string;
  timestamp: string;
  filename: string;
  count: number;
}

export function useDashboard() {
  const articles = ref<Article[]>([]);
  const recentAudit = ref<AuditEntry[]>([]);
  const loading = ref(true);
  const error = ref<string | null>(null);

  const counts = computed<StatusCounts>(() => {
    const all = articles.value;
    return {
      total: all.length,
      imported: all.filter((a) => a.status === 'imported').length,
      working: all.filter((a) => a.status === 'working').length,
      included: all.filter((a) => a.status === 'included').length,
      rejected: all.filter((a) => a.status === 'rejected').length,
    };
  });

  const screeningProgress = computed<ScreeningProgress>(() => {
    const total = articles.value.length;
    const screened = articles.value.filter((a) => a.aiDecision !== null).length;
    const percentage = total > 0 ? Math.round((screened / total) * 100) : 0;
    return { screened, total, percentage };
  });

  const hasArticles = computed(() => articles.value.length > 0);

  /** Merged timeline: import activities (with correct counts from SQL) + other audit entries */
  const groupedAudit = computed<GroupedAuditEntry[]>(() => {
    // Convert non-import audit entries (already excludes imports from backend)
    const nonImport: GroupedAuditEntry[] = recentAudit.value.map((entry) => ({
      id: entry.id,
      action: entry.action,
      source: entry.source,
      timestamp: entry.timestamp,
      details: entry.details,
    }));

    // Merge with import activities (already aggregated with correct counts at SQL level)
    const merged: GroupedAuditEntry[] = [
      ...importActivities.value.map(
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

    // Sort newest first
    merged.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
    return merged;
  });

  const importActivities = ref<ImportActivity[]>([]);

  async function refresh(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const [fetchedArticles, fetchedAudit, fetchedImports] = await Promise.all([
        tauriCommand<Article[]>('get_articles'),
        tauriCommand<AuditEntry[]>('get_recent_audit_entries', { limit: 10 }),
        tauriCommand<ImportActivity[]>('get_import_activities', { limit: 10 }),
      ]);
      articles.value = fetchedArticles;
      recentAudit.value = fetchedAudit;
      importActivities.value = fetchedImports;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  onMounted(refresh);

  return {
    articles,
    counts,
    screeningProgress,
    recentAudit,
    groupedAudit,
    loading,
    error,
    hasArticles,
    refresh,
  };
}

export function formatStatusLabel(status: ArticleStatus): string {
  const LABELS: Record<ArticleStatus, string> = {
    imported: 'Imported',
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
