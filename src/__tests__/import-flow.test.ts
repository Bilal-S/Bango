import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock nextPaint so we don't need real rAF in the test
vi.mock('@/utils/next-paint', () => ({
  nextPaint: vi.fn().mockResolvedValue(undefined),
}));

// Mock tauri command
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { nextPaint } from '@/utils/next-paint';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useImport } from '@/composables/use-import';
import type { ImportPreview, ImportResult } from '@/composables/use-import';

const mockPreview: ImportPreview = {
  totalRecords: 3,
  validRecords: 3,
  errorCount: 0,
  duplicateCount: 0,
  errors: [],
  errorGroups: [],
  previewArticles: [
    {
      title: 'Article A',
      authors: ['Author 1'],
      publicationYear: 2023,
      journal: 'Journal of Testing',
      doi: '10.1234/test',
    },
    {
      title: 'Article B',
      authors: ['Author 2'],
      publicationYear: 2022,
      journal: null,
      doi: null,
    },
    {
      title: 'Article C',
      authors: ['Author 3'],
      publicationYear: null,
      journal: null,
      doi: null,
    },
  ],
};

const mockImportResult: ImportResult = {
  importedCount: 3,
  skippedCount: 0,
  skippedByUser: 0,
  articles: [],
  remainingCapacity: 4997,
  validationErrors: [],
  errorGroups: [],
};

describe('useImport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ── Initial state ──────────────────────────────────────────────

  describe('initial state', () => {
    it('starts at upload step', () => {
      const { step } = useImport();
      expect(step.value).toBe('upload');
    });

    it('has no file loaded', () => {
      const { hasFile, fileName } = useImport();
      expect(hasFile.value).toBe(false);
      expect(fileName.value).toBeNull();
    });

    it('is not loading', () => {
      const { loading } = useImport();
      expect(loading.value).toBe(false);
    });

    it('has no error', () => {
      const { error } = useImport();
      expect(error.value).toBeNull();
    });

    it('has no preview', () => {
      const { preview } = useImport();
      expect(preview.value).toBeNull();
    });

    it('cannot import', () => {
      const { canImport } = useImport();
      expect(canImport.value).toBe(false);
    });
  });

  // ── loadFile ───────────────────────────────────────────────────

  describe('loadFile', () => {
    it('reads file content and advances to parse step', async () => {
      const { loadFile, step, fileName, hasFile } = useImport();

      const file = new File(['TY  - JOUR\nTI  - Test\nER  -\n'], 'test.ris', {
        type: 'text/plain',
      });
      await loadFile(file);

      expect(fileName.value).toBe('test.ris');
      expect(hasFile.value).toBe(true);
      expect(step.value).toBe('parse');
    });

    it('sets error on read failure', async () => {
      const { loadFile, error } = useImport();

      // Create a file that throws on .text()
      const badFile = new File(['content'], 'bad.ris');
      vi.spyOn(badFile, 'text').mockRejectedValue(new Error('Read error'));

      await loadFile(badFile);
      expect(error.value).toBe('Read error');
    });
  });

  // ── parseFile ──────────────────────────────────────────────────

  describe('parseFile', () => {
    it('parses RIS content and advances to import step', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockPreview);

      const imp = useImport();
      // Load a file first so there's content to parse
      const file = new File(['RIS content'], 'test.ris');
      await imp.loadFile(file);

      await imp.parseFile();

      expect(tauriCommand).toHaveBeenCalledWith('parse_ris_file', {
        request: {
          content: 'RIS content',
          filePath: null,
          fileName: 'test.ris',
        },
      });
      expect(imp.step.value).toBe('import');
      expect(imp.preview.value).toEqual(mockPreview);
      expect(imp.loading.value).toBe(false);
    });

    it('computes hasErrors when preview has errors', async () => {
      const previewWithErrors: ImportPreview = {
        ...mockPreview,
        errorCount: 2,
        errors: [
          { recordIndex: 0, message: 'Missing title' },
          { recordIndex: 1, message: 'Missing abstract' },
        ],
      };
      vi.mocked(tauriCommand).mockResolvedValue(previewWithErrors);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);
      await imp.parseFile();

      expect(imp.hasErrors.value).toBe(true);
    });

    it('returns early if no file is loaded', async () => {
      const imp = useImport();
      await imp.parseFile();

      expect(tauriCommand).not.toHaveBeenCalled();
    });

    it('sets error on parse failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Parse error'));

      const imp = useImport();
      const file = new File(['bad'], 'test.ris');
      await imp.loadFile(file);

      await imp.parseFile();

      expect(imp.error.value).toBe('Parse error');
      expect(imp.loading.value).toBe(false);
    });
  });

  // ── confirmImport ──────────────────────────────────────────────

  describe('confirmImport', () => {
    it('calls nextPaint before backend to ensure spinner paints', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockImportResult);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();

      // nextPaint must have been called
      expect(nextPaint).toHaveBeenCalled();

      // And nextPaint must be called BEFORE tauriCommand
      const npOrder = vi.mocked(nextPaint).mock.invocationCallOrder;
      const tcOrder = vi.mocked(tauriCommand).mock.invocationCallOrder;
      expect(npOrder.length).toBeGreaterThan(0);
      expect(tcOrder.length).toBeGreaterThan(0);
      expect(npOrder[0] as number).toBeLessThan(tcOrder[0] as number);
    });

    it('sets loading=true immediately before nextPaint', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockImportResult);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      // Track when loading was set
      const loadingStates: boolean[] = [];
      const originalNextPaint = vi.mocked(nextPaint);
      originalNextPaint.mockImplementationOnce(async () => {
        loadingStates.push(imp.loading.value);
      });

      await imp.confirmImport();

      // loading was true when nextPaint was called
      expect(loadingStates).toContain(true);
    });

    it('calls import_ris_file with correct payload', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockImportResult);

      const imp = useImport();
      const file = new File(['RIS content'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();

      expect(tauriCommand).toHaveBeenCalledWith('import_ris_file', {
        request: {
          content: 'RIS content',
          filePath: null,
          fileName: 'test.ris',
          excludedIndices: [],
        },
      });
    });

    it('sends excludedIndices when articles are removed', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockImportResult);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      imp.removeArticle(1);
      imp.removeArticle(2);

      await imp.confirmImport();

      const allCalls = vi.mocked(tauriCommand).mock.calls;
      const importCallIdx = allCalls.findIndex((c) => c[0] === 'import_ris_file');
      expect(importCallIdx).toBeGreaterThanOrEqual(0);
      const callArgs = allCalls[importCallIdx]!;
      const req = (callArgs[1] as Record<string, unknown>).request as {
        excludedIndices: number[];
      };
      expect(req.excludedIndices).toContain(1);
      expect(req.excludedIndices).toContain(2);
    });

    it('stores import result and advances to complete step', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockImportResult);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();

      expect(imp.importResult.value).toEqual(mockImportResult);
      expect(imp.step.value).toBe('complete');
    });

    it('sets loading=false in finally block on success', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockImportResult);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();
      expect(imp.loading.value).toBe(false);
    });

    it('sets loading=false in finally block on failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Import failed'));

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();
      expect(imp.loading.value).toBe(false);
    });

    it('sets error on import failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Import failed'));

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();
      expect(imp.error.value).toBe('Import failed');
      expect(imp.step.value).toBe('parse'); // should NOT advance
    });

    it('returns early if no file is loaded', async () => {
      const imp = useImport();
      await imp.confirmImport();

      expect(tauriCommand).not.toHaveBeenCalled();
      expect(nextPaint).not.toHaveBeenCalled();
    });

    it('dedup check failure is non-fatal', async () => {
      // import succeeds, but check_duplicates fails
      vi.mocked(tauriCommand)
        .mockResolvedValueOnce(mockImportResult)
        .mockRejectedValueOnce(new Error('Dedup failed'));

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);

      await imp.confirmImport();

      // Should still complete successfully
      expect(imp.step.value).toBe('complete');
      expect(imp.error.value).toBeNull();
    });
  });

  // ── removeArticle / computed ───────────────────────────────────

  describe('removeArticle and visibleArticles', () => {
    it('removes an article from visible list', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockPreview);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);
      await imp.parseFile();

      expect(imp.visibleArticles.value.length).toBe(3);

      imp.removeArticle(1);
      expect(imp.visibleArticles.value.length).toBe(2);
      expect(imp.visibleArticles.value[0]!.title).toBe('Article A');
      expect(imp.visibleArticles.value[1]!.title).toBe('Article C');
    });

    it('computes visibleCount correctly', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockPreview);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);
      await imp.parseFile();

      expect(imp.visibleCount.value).toBe(3);

      imp.removeArticle(0);
      imp.removeArticle(2);
      expect(imp.visibleCount.value).toBe(1);
    });
  });

  // ── reset ──────────────────────────────────────────────────────

  describe('reset', () => {
    it('clears all state back to initial', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockPreview);

      const imp = useImport();
      const file = new File(['RIS'], 'test.ris');
      await imp.loadFile(file);
      await imp.parseFile();
      imp.removeArticle(0);

      imp.reset();

      expect(imp.step.value).toBe('upload');
      expect(imp.fileName.value).toBeNull();
      expect(imp.preview.value).toBeNull();
      expect(imp.importResult.value).toBeNull();
      expect(imp.loading.value).toBe(false);
      expect(imp.error.value).toBeNull();
      expect(imp.visibleCount.value).toBe(0);
    });
  });
});

describe('ImportPreview duplicate signal', () => {
  it('preview carries duplicateCount from the backend', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ ...mockPreview, duplicateCount: 2 });

    const imp = useImport();
    const file = new File(['RIS'], 'test.ris');
    await imp.loadFile(file);
    await imp.parseFile();

    expect(imp.preview.value?.duplicateCount).toBe(2);
  });
});
