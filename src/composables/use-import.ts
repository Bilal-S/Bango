import { ref, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface ImportPreview {
  totalRecords: number;
  validRecords: number;
  errorCount: number;
  errors: ImportError[];
  previewArticles: PreviewArticle[];
}

export interface ImportError {
  recordIndex: number;
  message: string;
}

export interface PreviewArticle {
  title: string;
  authors: string[];
  publicationYear: number | null;
  journal: string | null;
  doi: string | null;
}

export interface ImportResult {
  importedCount: number;
  articles: unknown[];
  remainingCapacity: number;
}

export type ImportStep = 'upload' | 'parse' | 'import' | 'complete';

export function useImport() {
  const step = ref<ImportStep>('upload');
  const fileName = ref<string | null>(null);
  const fileContent = ref<string | null>(null);
  const preview = ref<ImportPreview | null>(null);
  const importResult = ref<ImportResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const hasFile = computed(() => fileContent.value !== null);
  const hasErrors = computed(() => (preview.value?.errorCount ?? 0) > 0);
  const canImport = computed(() => preview.value !== null && preview.value.errorCount === 0);

  async function loadFile(file: File): Promise<void> {
    loading.value = true;
    error.value = null;
    fileName.value = file.name;

    try {
      fileContent.value = await file.text();
      step.value = 'parse';
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Failed to read file';
    } finally {
      loading.value = false;
    }
  }

  async function parseFile(): Promise<void> {
    if (!fileContent.value || !fileName.value) return;

    loading.value = true;
    error.value = null;

    try {
      preview.value = await tauriCommand<ImportPreview>('parse_ris_file', {
        request: {
          content: fileContent.value,
          fileName: fileName.value,
        },
      });
      step.value = 'import';
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Parse failed';
    } finally {
      loading.value = false;
    }
  }

  async function confirmImport(): Promise<void> {
    if (!fileContent.value || !fileName.value) return;

    loading.value = true;
    error.value = null;

    try {
      importResult.value = await tauriCommand<ImportResult>('import_ris_file', {
        request: {
          content: fileContent.value,
          fileName: fileName.value,
        },
      });
      step.value = 'complete';
    } catch (e) {
      error.value = e instanceof Error ? e.message : 'Import failed';
    } finally {
      loading.value = false;
    }
  }

  function reset(): void {
    step.value = 'upload';
    fileName.value = null;
    fileContent.value = null;
    preview.value = null;
    importResult.value = null;
    loading.value = false;
    error.value = null;
  }

  return {
    step,
    fileName,
    preview,
    importResult,
    loading,
    error,
    hasFile,
    hasErrors,
    canImport,
    loadFile,
    parseFile,
    confirmImport,
    reset,
  };
}
