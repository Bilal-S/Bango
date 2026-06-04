import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri dialog
vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

// Mock tauri command
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

// Mock all stores to avoid Pinia setup
vi.mock('@/stores/articles', () => ({
  useArticlesStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/stores/criteria', () => ({
  useCriteriaStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/stores/tags', () => ({
  useTagsStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/stores/labels', () => ({
  useLabelsStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/stores/llm-config', () => ({
  useLlmConfigStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/stores/audit', () => ({
  useAuditStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/stores/screening', () => ({
  useScreeningStore: vi.fn(() => ({
    invalidate: vi.fn(),
    fetchIfNeeded: vi.fn().mockResolvedValue(undefined),
  })),
}));
vi.mock('@/composables/use-summary', () => ({
  useSummary: vi.fn(() => ({ clearSummary: vi.fn() })),
}));

import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useExport } from '@/composables/use-export';

describe('useExport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('is not exporting', () => {
      const { exporting } = useExport();
      expect(exporting.value).toBe(false);
    });

    it('has no error', () => {
      const { error } = useExport();
      expect(error.value).toBeNull();
    });
  });

  describe('exportRis', () => {
    it('calls save dialog and tauri command on success', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/export.ris');
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { exportRis, exporting, error } = useExport();
      const result = await exportRis();

      expect(result).toBe(true);
      expect(save).toHaveBeenCalledWith({
        defaultPath: 'included-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      expect(tauriCommand).toHaveBeenCalledWith('export_ris_to_file', {
        path: '/path/to/export.ris',
      });
      expect(exporting.value).toBe(false);
      expect(error.value).toBeNull();
    });

    it('returns false when user cancels save dialog', async () => {
      vi.mocked(save).mockResolvedValue(null);

      const { exportRis } = useExport();
      const result = await exportRis();

      expect(result).toBe(false);
      expect(tauriCommand).not.toHaveBeenCalled();
    });

    it('sets error on tauri command failure', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/export.ris');
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Export failed'));

      const { exportRis, error, exporting } = useExport();
      const result = await exportRis();

      expect(result).toBe(false);
      expect(error.value).toBe('Export failed');
      expect(exporting.value).toBe(false);
    });

    it('handles non-Error exceptions in exportRis', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/export.ris');
      vi.mocked(tauriCommand).mockRejectedValue('string error');

      const { exportRis, error } = useExport();
      await exportRis();

      expect(error.value).toBe('string error');
    });

    it('sets exporting=true during operation', async () => {
      let resolveCmd: () => void;
      const cmdPromise = new Promise<void>((r) => {
        resolveCmd = r;
      });
      vi.mocked(save).mockResolvedValue('/path.ris');
      vi.mocked(tauriCommand).mockReturnValue(cmdPromise);

      const { exportRis, exporting } = useExport();
      const promise = exportRis();

      expect(exporting.value).toBe(true);
      resolveCmd!();
      await promise;

      expect(exporting.value).toBe(false);
    });
  });

  describe('exportProject', () => {
    it('calls save dialog and tauri command on success', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/project.bango.json');
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { exportProject, error } = useExport();
      const result = await exportProject();

      expect(result).toBe(true);
      expect(save).toHaveBeenCalledWith({
        defaultPath: 'bango-project.bango.json',
        filters: [
          { name: 'Bango Backup', extensions: ['bango.json'] },
          { name: 'JSON', extensions: ['json'] },
        ],
      });
      expect(tauriCommand).toHaveBeenCalledWith('export_project_to_file', {
        path: '/path/to/project.bango.json',
      });
      expect(error.value).toBeNull();
    });

    it('returns false when user cancels', async () => {
      vi.mocked(save).mockResolvedValue(null);

      const { exportProject } = useExport();
      const result = await exportProject();

      expect(result).toBe(false);
      expect(tauriCommand).not.toHaveBeenCalled();
    });

    it('sets error on failure', async () => {
      vi.mocked(save).mockResolvedValue('/path.bango.json');
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Backup failed'));

      const { exportProject, error } = useExport();
      await exportProject();

      expect(error.value).toBe('Backup failed');
    });
  });

  describe('importProject', () => {
    it('reads file and calls import command then refreshes stores', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { importProject, exporting, error } = useExport();
      const file = new File(['{"project":"data"}'], 'backup.bango.json');
      await importProject(file);

      expect(tauriCommand).toHaveBeenCalledWith('import_project_backup', {
        request: { jsonContent: '{"project":"data"}' },
      });
      expect(exporting.value).toBe(false);
      expect(error.value).toBeNull();
    });

    it('sets error on import failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Invalid backup'));

      const { importProject, error } = useExport();
      const file = new File(['bad data'], 'backup.bango.json');
      await importProject(file);

      expect(error.value).toBe('Invalid backup');
    });
  });

  describe('resetProject', () => {
    it('calls reset command and invalidates stores', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { resetProject, error, exporting } = useExport();
      const result = await resetProject();

      expect(result).toBe(true);
      expect(tauriCommand).toHaveBeenCalledWith('reset_project');
      expect(exporting.value).toBe(false);
      expect(error.value).toBeNull();
    });

    it('sets error on reset failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Reset failed'));

      const { resetProject, error } = useExport();
      const result = await resetProject();

      expect(result).toBe(false);
      expect(error.value).toBe('Reset failed');
    });
  });
});
