import { ref } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from './use-tauri-command';

export function useExport() {
  const exporting = ref(false);
  const error = ref<string | null>(null);

  async function exportRis(): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const filePath = await save({
        defaultPath: 'included-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await tauriCommand('export_ris_to_file', { path: filePath });
      }
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  async function exportProject(): Promise<void> {
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
      }
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
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
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  async function resetProject(): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      await tauriCommand('reset_project');
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  return { exporting, error, exportRis, exportProject, importProject, resetProject };
}
