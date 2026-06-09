import { ref } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from './use-tauri-command';
import { useArticlesStore } from '@/stores/articles';
import { useCriteriaStore } from '@/stores/criteria';
import { useTagsStore } from '@/stores/tags';
import { useLabelsStore } from '@/stores/labels';
import { useLlmConfigStore } from '@/stores/llm-config';
import { useAuditStore } from '@/stores/audit';
import { useScreeningStore } from '@/stores/screening';
import { useSummary } from './use-summary';

export function useExport() {
  const exporting = ref(false);
  const error = ref<string | null>(null);

  function invalidateAllStores(): void {
    useArticlesStore().invalidate();
    useCriteriaStore().invalidate();
    useTagsStore().invalidate();
    useLabelsStore().invalidate();
    useLlmConfigStore().invalidate();
    useAuditStore().invalidate();
    useScreeningStore().invalidate();
  }

  /** Invalidate all stores and then proactively re-fetch so data is immediately available. */
  async function refreshAllStores(): Promise<void> {
    invalidateAllStores();
    await Promise.all([
      useArticlesStore().fetchIfNeeded(),
      useCriteriaStore().fetchIfNeeded(),
      useTagsStore().fetchIfNeeded(),
      useLabelsStore().fetchIfNeeded(),
      useLlmConfigStore().fetchIfNeeded(),
      useAuditStore().fetchIfNeeded(),
      useScreeningStore().fetchIfNeeded(),
    ]);
  }

  async function exportRis(): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({
        defaultPath: 'included-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await tauriCommand('export_ris_to_file', { path: filePath });
        return true;
      }
      return false;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  async function exportRisForTab(
    status: string,
    screeningErrorsOnly: boolean,
    label: string
  ): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const slug = label.toLowerCase().replace(/\s+/g, '-');
      const filePath = await save({
        defaultPath: `${slug}-articles.ris`,
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await tauriCommand('export_ris_for_tab_to_file', {
          path: filePath,
          status,
          screeningErrorsOnly,
        });
        return true;
      }
      return false;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  async function exportProject(): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({
        defaultPath: 'bango-project.bango.json',
        filters: [
          { name: 'Bango Backup', extensions: ['bango.json'] },
          { name: 'JSON', extensions: ['json'] },
        ],
      });
      if (filePath) {
        await tauriCommand('export_project_to_file', { path: filePath });
        return true;
      }
      return false;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  async function importProject(file: File): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      console.log('[import] Reading file:', file.name, 'size:', file.size);
      const content = await file.text();
      console.log(
        '[import] File content length:',
        content.length,
        'first 200 chars:',
        content.substring(0, 200)
      );
      console.log('[import] Calling import_project_backup...');
      await tauriCommand('import_project_backup', {
        request: { jsonContent: content },
      });
      console.log('[import] Tauri command succeeded. Refreshing stores...');
      await refreshAllStores();
      useSummary().clearSummary();
      console.log('[import] Stores refreshed. Import complete.');
      return true;
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error('[import] Import failed:', msg, e);
      error.value = msg;
      return false;
    } finally {
      exporting.value = false;
    }
  }

  async function resetProject(): Promise<boolean> {
    exporting.value = true;
    error.value = null;
    try {
      await tauriCommand('reset_project');
      invalidateAllStores();
      useSummary().clearSummary();
      return true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    } finally {
      exporting.value = false;
    }
  }

  return {
    exporting,
    error,
    exportRis,
    exportRisForTab,
    exportProject,
    importProject,
    resetProject,
  };
}
