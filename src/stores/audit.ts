import { defineStore } from 'pinia';
import { ref } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import type { AuditEntry } from '@/types';

/** Shape returned by the Rust `get_import_activities` command */
export interface ImportActivity {
  id: string;
  timestamp: string;
  filename: string;
  count: number;
}

export const useAuditStore = defineStore('audit', () => {
  const recentAudit = ref<AuditEntry[]>([]);
  const importActivities = ref<ImportActivity[]>([]);
  const loading = ref(false);
  const initialized = ref(false);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetch();
  }

  async function fetch(): Promise<void> {
    loading.value = true;
    try {
      const [audit, imports] = await Promise.all([
        tauriCommand<AuditEntry[]>('get_recent_audit_entries', { limit: 10 }),
        tauriCommand<ImportActivity[]>('get_import_activities', { limit: 10 }),
      ]);
      recentAudit.value = audit;
      importActivities.value = imports;
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  function invalidate(): void {
    initialized.value = false;
  }

  return { recentAudit, importActivities, loading, initialized, fetchIfNeeded, fetch, invalidate };
});
