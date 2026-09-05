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

import { tauriCommand } from '@/composables/use-tauri-command';
import { useImport, type ImportPreview, type ImportResult } from '@/composables/use-import';
import { useZotero } from '@/composables/use-zotero';
import type { ZoteroConnectionStatus } from '@/types/zotero';

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

function status(status: ZoteroConnectionStatus['status'], hint: string | null = null) {
  return {
    status,
    apiVersion: status === 'ok' ? '3' : null,
    zoteroVersion: status === 'ok' ? '10.0.1' : null,
    serverId: status === 'ok' ? 'SID1' : null,
    hint,
  };
}

describe('Zotero import flow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('zotero_step_transitions_on_connection_ok', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(status('ok'));
    const zotero = useZotero();
    const imp = useImport();

    const ok = await zotero.checkConnection();
    expect(ok).toBe(true);
    expect(zotero.connectionMessage.value).toBeNull();

    // The wizard's onZoteroSelected handler: enter the step only on ok.
    if (ok) imp.startZoteroImport();
    expect(imp.step.value).toBe('zotero');
    expect(imp.importSource.value).toBe('zotero');
  });

  it('zotero_step_shows_not_running_message', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(status('not_running', 'Start Zotero and try again.'));
    const zotero = useZotero();
    const ok = await zotero.checkConnection();
    expect(ok).toBe(false);
    expect(zotero.connectionMessage.value).toContain('Zotero is not running');
  });

  it('zotero_step_shows_api_disabled_message', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(
      status(
        'api_disabled',
        'Enable the local API in Zotero under Settings -> Advanced -> "Allow other applications on this computer to communicate with Zotero", then try again.'
      )
    );
    const zotero = useZotero();
    const ok = await zotero.checkConnection();
    expect(ok).toBe(false);
    expect(zotero.connectionMessage.value).toContain('Settings -> Advanced');
    expect(zotero.connectionMessage.value).toContain('Allow other applications');
  });

  it('zotero_step_shows_error_status_message', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(status('error', 'Zotero request failed: HTTP 500'));
    const zotero = useZotero();
    const ok = await zotero.checkConnection();
    expect(ok).toBe(false);
    expect(zotero.connectionMessage.value).toContain('HTTP 500');
  });

  it('confirm_import_maps_removed_indices_to_excluded_keys', async () => {
    const zoteroResult = {
      result: mockImportResult,
      attachedCount: 2,
      attachmentFailedCount: 0,
      attachmentSkippedCount: 0,
    };
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'import_zotero_collection') return Promise.resolve(zoteroResult);
      if (cmd === 'check_duplicates') return Promise.reject(new Error('non-fatal'));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });

    const imp = useImport();
    imp.startZoteroImport();
    imp.applyZoteroPreview({
      collectionKey: 'KEY1',
      collectionName: 'Super Collection',
      preview: mockPreview,
      articleKeys: ['K1', 'K2', 'K3'],
      libraryVersion: 15,
      totalItems: 3,
      attachmentCount: 2,
      tagCount: 4,
    });
    expect(imp.step.value).toBe('import');

    // Deselect the middle row; removal maps to the Zotero item key K2.
    imp.removeArticle(1);
    await imp.confirmImport();

    expect(tauriCommand).toHaveBeenCalledWith('import_zotero_collection', {
      collectionKey: 'KEY1',
      excludedKeys: ['K2'],
      expectedLibraryVersion: 15,
    });
    expect(imp.step.value).toBe('complete');
    expect(imp.importResult.value).toEqual(mockImportResult);
    expect(imp.zoteroAttachmentSummary.value).toEqual({
      attachedCount: 2,
      failedCount: 0,
      skippedCount: 0,
    });
  });

  it('zero_valid_records_renders_empty_state', () => {
    const imp = useImport();
    imp.startZoteroImport();
    imp.applyZoteroPreview({
      collectionKey: 'KEY1',
      collectionName: 'Empty Collection',
      preview: {
        ...mockPreview,
        validRecords: 0,
        previewArticles: [],
      },
      articleKeys: [],
      libraryVersion: 15,
      totalItems: 2,
      attachmentCount: 0,
      tagCount: 0,
    });

    // Distinct 0-importable state: Confirm stays dead, Back stays enabled.
    expect(imp.hasZeroValid.value).toBe(true);
    expect(imp.canImport.value).toBe(false);
  });

  it('apply_zotero_preview_carries_duplicate_count', () => {
    const imp = useImport();
    imp.startZoteroImport();
    imp.applyZoteroPreview({
      collectionKey: 'KEY1',
      collectionName: 'Super Collection',
      preview: { ...mockPreview, duplicateCount: 2 },
      articleKeys: ['K1', 'K2', 'K3'],
      libraryVersion: 15,
      totalItems: 3,
      attachmentCount: 2,
      tagCount: 4,
    });

    // The review step's Duplicates stat reads preview.duplicateCount.
    expect(imp.preview.value?.duplicateCount).toBe(2);
  });
});

describe('Zotero import stale-preview guard', () => {
  it('stale_preview_confirm_returns_to_picker', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('should not be called'));
    const imp = useImport();
    imp.startZoteroImport();
    // No applyZoteroPreview: no collection key / library version.

    await imp.confirmImport();

    expect(tauriCommand).not.toHaveBeenCalledWith('import_zotero_collection', expect.anything());
    expect(imp.step.value).toBe('zotero');
    expect(imp.error.value).toContain('stale');
  });
});
