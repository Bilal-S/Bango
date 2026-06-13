import { ref, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { DedupResult } from './use-dedup';
import { nextPaint } from '@/utils/next-paint';

export interface ErrorGroup {
  message: string;
  count: number;
  recordIndices: number[];
}

export interface ImportError {
  recordIndex: number;
  message: string;
}

export interface ImportPreview {
  totalRecords: number;
  validRecords: number;
  errorCount: number;
  errors: ImportError[];
  errorGroups: ErrorGroup[];
  previewArticles: PreviewArticle[];
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
  skippedCount: number;
  skippedByUser: number;
  articles: unknown[];
  remainingCapacity: number;
  validationErrors: ImportError[];
  errorGroups: ErrorGroup[];
}

type ImportFormat = 'ris' | 'bibtex';
export type ImportStep = 'upload' | 'parse' | 'import' | 'complete';

/** Detect import format from file extension. */
function detectFormat(fileName: string): ImportFormat {
  const ext = fileName.toLowerCase();
  if (ext.endsWith('.bib') || ext.endsWith('.bibtex')) return 'bibtex';
  return 'ris';
}

export function useImport() {
  const step = ref<ImportStep>('upload');
  const fileName = ref<string | null>(null);
  const fileContent = ref<string | null>(null);
  const filePath = ref<string | null>(null);
  const preview = ref<ImportPreview | null>(null);
  const importResult = ref<ImportResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const removedIndices = ref<Set<number>>(new Set());
  const dedupSummary = ref<DedupResult | null>(null);

  const hasFile = computed(() => fileContent.value !== null || filePath.value !== null);
  const hasErrors = computed(() => (preview.value?.errorCount ?? 0) > 0);
  const canImport = computed(() => preview.value !== null && preview.value.validRecords > 0);

  const visibleArticles = computed(() => {
    if (!preview.value) return [];
    return preview.value.previewArticles.filter((_, i) => !removedIndices.value.has(i));
  });

  const visibleCount = computed(() => {
    if (!preview.value) return 0;
    return preview.value.validRecords - removedIndices.value.size;
  });

  function removeArticle(index: number): void {
    removedIndices.value = new Set([...removedIndices.value, index]);
  }

  async function loadFile(file: File): Promise<void> {
    loading.value = true;
    error.value = null;
    fileName.value = file.name;

    try {
      fileContent.value = await file.text();
      filePath.value = null;
      step.value = 'parse';
    } catch (e) {
      console.error('[import] loadFile failed:', e);
      error.value = e instanceof Error ? e.message : String(e) || 'Failed to read file';
    } finally {
      loading.value = false;
    }
  }

  function loadFilePath(path: string, name: string): void {
    fileName.value = name;
    filePath.value = path;
    fileContent.value = null;
    step.value = 'parse';
  }

  async function parseFile(): Promise<void> {
    if ((!fileContent.value && !filePath.value) || !fileName.value) return;

    loading.value = true;
    error.value = null;

    try {
      const cmd =
        detectFormat(fileName.value) === 'bibtex' ? 'parse_bibtex_file' : 'parse_ris_file';
      preview.value = await tauriCommand<ImportPreview>(cmd, {
        request: {
          content: fileContent.value,
          filePath: filePath.value,
          fileName: fileName.value,
        },
      });
      step.value = 'import';
    } catch (e) {
      console.error('[import] parseFile failed:', e);
      error.value = e instanceof Error ? e.message : String(e) || 'Parse failed';
    } finally {
      loading.value = false;
    }
  }

  async function confirmImport(): Promise<void> {
    if ((!fileContent.value && !filePath.value) || !fileName.value) return;

    loading.value = true;
    error.value = null;

    // Yield to the browser so the spinner paints before the blocking IPC call
    await nextPaint();

    try {
      const cmd =
        detectFormat(fileName.value) === 'bibtex' ? 'import_bibtex_file' : 'import_ris_file';
      importResult.value = await tauriCommand<ImportResult>(cmd, {
        request: {
          content: fileContent.value,
          filePath: filePath.value,
          fileName: fileName.value,
          excludedIndices: [...removedIndices.value],
        },
      });

      // Run duplicate detection (no merge) so we can show a summary
      try {
        dedupSummary.value = await tauriCommand<DedupResult>('check_duplicates');
      } catch {
        // Non-fatal - dedup summary is optional
      }

      step.value = 'complete';
    } catch (e) {
      console.error('[import] confirmImport failed:', e);
      error.value = e instanceof Error ? e.message : String(e) || 'Import failed';
    } finally {
      loading.value = false;
    }
  }

  function reset(): void {
    step.value = 'upload';
    fileName.value = null;
    fileContent.value = null;
    filePath.value = null;
    preview.value = null;
    importResult.value = null;
    dedupSummary.value = null;
    loading.value = false;
    error.value = null;
    removedIndices.value = new Set();
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
    removedIndices,
    visibleArticles,
    visibleCount,
    dedupSummary,
    loadFile,
    loadFilePath,
    parseFile,
    confirmImport,
    removeArticle,
    reset,
  };
}
