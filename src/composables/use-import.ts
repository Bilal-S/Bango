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
  /** Valid records whose canonical DOI already exists in the library. */
  duplicateCount: number;
  /** Indices into the valid records that duplicate the library (the Skip-box
   * count and the confirm-button arithmetic). */
  duplicateIndices: number[];
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
  /** Library duplicates dropped before import (review-step Skip checkbox). */
  skippedDuplicates: number;
  skippedByUser: number;
  articles: unknown[];
  remainingCapacity: number;
  validationErrors: ImportError[];
  errorGroups: ErrorGroup[];
}

type ImportFormat = 'ris' | 'bibtex';
export type ImportStep = 'upload' | 'parse' | 'zotero' | 'import' | 'complete';

/** Attachment tallies reported by the Zotero import command. */
export interface ZoteroAttachmentSummary {
  attachedCount: number;
  failedCount: number;
  skippedCount: number;
}

/** Zotero-specific preview metadata shown on the review + complete steps. */
export interface ZoteroPreviewMeta {
  collectionName: string;
  totalItems: number;
  attachmentCount: number;
  tagCount: number;
}

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
  // Review-step Skip checkbox: drop records whose canonical DOI already
  // exists in the library before import (within-file duplicates and every
  // other strategy still flow to the classify phase). Defaults to on and
  // resets for every new preview.
  const skipDuplicates = ref(true);

  // Zotero flow state (keyed by Zotero item key, never positional).
  const importSource = ref<'file' | 'zotero'>('file');
  const zoteroCollectionKey = ref<string | null>(null);
  const zoteroArticleKeys = ref<string[]>([]);
  const zoteroLibraryVersion = ref<number | null>(null);
  const zoteroPreviewMeta = ref<ZoteroPreviewMeta | null>(null);
  const zoteroAttachmentSummary = ref<ZoteroAttachmentSummary | null>(null);

  const hasFile = computed(() => fileContent.value !== null || filePath.value !== null);
  const hasErrors = computed(() => (preview.value?.errorCount ?? 0) > 0);
  const canImport = computed(() => preview.value !== null && preview.value.validRecords > 0);
  /** Distinct "0 importable items" state (Back enabled, never a dead Confirm). */
  const hasZeroValid = computed(() => preview.value !== null && preview.value.validRecords === 0);

  const visibleArticles = computed(() => {
    if (!preview.value) return [];
    return preview.value.previewArticles.filter((_, i) => !removedIndices.value.has(i));
  });

  const visibleCount = computed(() => {
    if (!preview.value) return 0;
    return preview.value.validRecords - removedIndices.value.size;
  });

  /** The confirm-button count: the visible articles minus the library
   * duplicates the Skip checkbox will drop (they come back when it is
   * unchecked; manually removed rows never count twice). */
  const importableCount = computed(() => {
    if (!preview.value) return 0;
    if (!skipDuplicates.value) return visibleCount.value;
    const duplicatesLeft = preview.value.duplicateIndices.filter(
      (index) => !removedIndices.value.has(index)
    ).length;
    return Math.max(visibleCount.value - duplicatesLeft, 0);
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

  /** Enter the Zotero step (connection already verified by the caller). */
  function startZoteroImport(): void {
    importSource.value = 'zotero';
    fileName.value = null;
    fileContent.value = null;
    filePath.value = null;
    preview.value = null;
    importResult.value = null;
    dedupSummary.value = null;
    error.value = null;
    removedIndices.value = new Set();
    skipDuplicates.value = true;
    step.value = 'zotero';
  }

  /** Apply a Zotero collection preview and advance to the review step. */
  function applyZoteroPreview(payload: {
    collectionKey: string;
    collectionName: string;
    preview: ImportPreview;
    articleKeys: string[];
    libraryVersion: number | null;
    totalItems: number;
    attachmentCount: number;
    tagCount: number;
  }): void {
    importSource.value = 'zotero';
    zoteroCollectionKey.value = payload.collectionKey;
    zoteroArticleKeys.value = payload.articleKeys;
    zoteroLibraryVersion.value = payload.libraryVersion;
    zoteroPreviewMeta.value = {
      collectionName: payload.collectionName,
      totalItems: payload.totalItems,
      attachmentCount: payload.attachmentCount,
      tagCount: payload.tagCount,
    };
    fileName.value = `Zotero: ${payload.collectionName}`;
    preview.value = payload.preview;
    importResult.value = null;
    dedupSummary.value = null;
    error.value = null;
    removedIndices.value = new Set();
    skipDuplicates.value = true;
    step.value = 'import';
  }

  /** Return to the Zotero picker (e.g. after a library-changed guard). */
  function backToZoteroPicker(): void {
    step.value = 'zotero';
    preview.value = null;
    importResult.value = null;
    dedupSummary.value = null;
    removedIndices.value = new Set();
    zoteroArticleKeys.value = [];
    zoteroLibraryVersion.value = null;
    zoteroPreviewMeta.value = null;
    zoteroAttachmentSummary.value = null;
    error.value = null;
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
      skipDuplicates.value = true;
      step.value = 'import';
    } catch (e) {
      console.error('[import] parseFile failed:', e);
      error.value = e instanceof Error ? e.message : String(e) || 'Parse failed';
    } finally {
      loading.value = false;
    }
  }

  async function confirmImport(): Promise<void> {
    if (importSource.value === 'zotero') {
      await confirmZoteroImport();
      return;
    }
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
          skipDuplicates: skipDuplicates.value,
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

  /** Zotero confirm: `removedIndices` map to `excludedKeys` via the stored
   *  Zotero item keys (key-based exclusion is immune to re-ordering). */
  async function confirmZoteroImport(): Promise<void> {
    if (!zoteroCollectionKey.value || zoteroLibraryVersion.value === null) {
      // A stale/incomplete preview must not silently no-op: send the user
      // back to the picker for a fresh one.
      backToZoteroPicker();
      error.value = 'The Zotero preview is stale - go back and re-select the collection.';
      return;
    }

    loading.value = true;
    error.value = null;

    // Yield to the browser so the spinner paints before the blocking IPC call
    await nextPaint();

    try {
      const excludedKeys = [...removedIndices.value]
        .map((index) => zoteroArticleKeys.value[index])
        .filter((key): key is string => typeof key === 'string');
      const result = await tauriCommand<import('@/types/zotero').ZoteroImportResult>(
        'import_zotero_collection',
        {
          collectionKey: zoteroCollectionKey.value,
          excludedKeys,
          expectedLibraryVersion: zoteroLibraryVersion.value,
          skipDuplicates: skipDuplicates.value,
        }
      );
      importResult.value = result.result;
      zoteroAttachmentSummary.value = {
        attachedCount: result.attachedCount,
        failedCount: result.attachmentFailedCount,
        skippedCount: result.attachmentSkippedCount,
      };

      // Run duplicate detection (no merge) so we can show a summary
      try {
        dedupSummary.value = await tauriCommand<DedupResult>('check_duplicates');
      } catch {
        // Non-fatal - dedup summary is optional
      }

      step.value = 'complete';
    } catch (e) {
      console.error('[import] confirmZoteroImport failed:', e);
      const message = e instanceof Error ? e.message : String(e) || 'Import failed';
      if (message.includes('changed since the preview')) {
        // Library-version guard fired: back to the picker for a fresh preview.
        backToZoteroPicker();
        error.value = 'Zotero changed since the preview - re-select the collection';
      } else {
        error.value = message;
      }
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
    importSource.value = 'file';
    zoteroCollectionKey.value = null;
    zoteroArticleKeys.value = [];
    zoteroLibraryVersion.value = null;
    zoteroPreviewMeta.value = null;
    zoteroAttachmentSummary.value = null;
    skipDuplicates.value = true;
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
    hasZeroValid,
    removedIndices,
    skipDuplicates,
    visibleArticles,
    visibleCount,
    importableCount,
    dedupSummary,
    importSource,
    zoteroCollectionKey,
    zoteroArticleKeys,
    zoteroLibraryVersion,
    zoteroPreviewMeta,
    zoteroAttachmentSummary,
    loadFile,
    loadFilePath,
    parseFile,
    confirmImport,
    startZoteroImport,
    applyZoteroPreview,
    backToZoteroPicker,
    removeArticle,
    reset,
  };
}
