import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import type { AuditEntry } from '@/types';

/** Shape returned by the Rust `get_import_activities` command */
export interface ImportActivity {
  id: string;
  timestamp: string;
  filename: string;
  count: number;
}

const PAGE_SIZE = 10;

export const useAuditStore = defineStore('audit', () => {
  const recentAudit = ref<AuditEntry[]>([]);
  const importActivities = ref<ImportActivity[]>([]);
  const loading = ref(false);
  const initialized = ref(false);

  // Pagination state
  const auditOffset = ref(0);
  const importOffset = ref(0);
  const hasMoreAudit = ref(true);
  const hasMoreImports = ref(true);
  const loadingMore = ref(false);

  /** Total items currently loaded (used to compute next offset) */
  const totalLoaded = computed(() => recentAudit.value.length + importActivities.value.length);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetch();
  }

  /** Initial fetch - resets all state */
  async function fetch(): Promise<void> {
    loading.value = true;
    try {
      const [audit, imports] = await Promise.all([
        tauriCommand<AuditEntry[]>('get_recent_audit_entries', { limit: PAGE_SIZE, offset: 0 }),
        tauriCommand<ImportActivity[]>('get_import_activities', { limit: PAGE_SIZE, offset: 0 }),
      ]);
      recentAudit.value = audit;
      importActivities.value = imports;
      auditOffset.value = audit.length;
      importOffset.value = imports.length;
      hasMoreAudit.value = audit.length === PAGE_SIZE;
      hasMoreImports.value = imports.length === PAGE_SIZE;
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  /** Load the next page of both audit entries and import activities, merged by timestamp */
  async function loadMore(): Promise<void> {
    if (loadingMore.value || (!hasMoreAudit.value && !hasMoreImports.value)) return;
    loadingMore.value = true;
    try {
      const promises: Promise<unknown>[] = [];

      if (hasMoreAudit.value) {
        promises.push(
          tauriCommand<AuditEntry[]>('get_recent_audit_entries', {
            limit: PAGE_SIZE,
            offset: auditOffset.value,
          }).then((entries) => {
            recentAudit.value = [...recentAudit.value, ...entries];
            auditOffset.value += entries.length;
            hasMoreAudit.value = entries.length === PAGE_SIZE;
          })
        );
      }

      if (hasMoreImports.value) {
        promises.push(
          tauriCommand<ImportActivity[]>('get_import_activities', {
            limit: PAGE_SIZE,
            offset: importOffset.value,
          }).then((imports) => {
            importActivities.value = [...importActivities.value, ...imports];
            importOffset.value += imports.length;
            hasMoreImports.value = imports.length === PAGE_SIZE;
          })
        );
      }

      await Promise.all(promises);
    } finally {
      loadingMore.value = false;
    }
  }

  function invalidate(): void {
    recentAudit.value = [];
    importActivities.value = [];
    initialized.value = false;
    auditOffset.value = 0;
    importOffset.value = 0;
    hasMoreAudit.value = true;
    hasMoreImports.value = true;
  }

  return {
    recentAudit,
    importActivities,
    loading,
    loadingMore,
    initialized,
    hasMoreAudit,
    hasMoreImports,
    totalLoaded,
    fetchIfNeeded,
    fetch,
    loadMore,
    invalidate,
  };
});
