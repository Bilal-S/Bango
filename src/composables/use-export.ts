import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export function useExport() {
  const exporting = ref(false);
  const error = ref<string | null>(null);

  async function exportRis(): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const risContent = await tauriCommand<string>('export_ris');
      downloadFile(risContent, 'included-articles.ris', 'application/x-research-info-systems');
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  async function exportProject(password: string): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const jsonContent = await tauriCommand<string>('export_project_backup', {
        request: { password },
      });
      downloadFile(jsonContent, 'bango-project.bango.json', 'application/json');
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      exporting.value = false;
    }
  }

  async function importProject(file: File, password: string): Promise<void> {
    exporting.value = true;
    error.value = null;
    try {
      const content = await file.text();
      await tauriCommand('import_project_backup', {
        request: { jsonContent: content, password },
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

function downloadFile(content: string, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
