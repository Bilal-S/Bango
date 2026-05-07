import { ref } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { tauriCommand } from './use-tauri-command';

export function useExport() {
  const exporting = ref(false);
  const error = ref<string | null>(null);

  async function exportRis(): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const risContent = await tauriCommand<string>('export_ris');
      const filePath = await save({
        defaultPath: 'included-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      if (filePath) {
        await writeTextFile(filePath, risContent);
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
      const jsonContent = await tauriCommand<string>('export_project_backup');
      const filePath = await save({
        defaultPath: 'bango-project.bango.json',
        filters: [
          { name: 'Bango Backup', extensions: ['bango.json'] },
          { name: 'JSON', extensions: ['json'] },
        ],
      });
      if (filePath) {
        await writeTextFile(filePath, jsonContent);
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
