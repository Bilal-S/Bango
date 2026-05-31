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

  async function importProject(file: File): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const content = await file.text();
      await tauriCommand('import_project_backup', {
        request: { jsonContent: content },
      });
      await refreshAllStores();
      useSummary().clearSummary();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
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

  return { exporting, error, exportRis, exportProject, importProject, resetProject };
}
