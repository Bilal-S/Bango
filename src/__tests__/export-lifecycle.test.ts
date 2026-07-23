import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Tauri dialog
vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

// Mock window.location so the success paths (import/reset) can run to
// completion in jsdom without actually reloading the test runner. `reload` and
// `hash` are captured per-test so assertions can verify the app navigates to
// the Dashboard (hash `#/`) before reloading (Option A: fresh-start landing).
const reloadMock = vi.fn();
const locationMock = { ...window.location, reload: reloadMock, hash: '' };
beforeEach(() => {
  reloadMock.mockReset();
  locationMock.hash = '';
  Object.defineProperty(window, 'location', {
    value: locationMock,
    writable: true,
  });
});

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

// Singleton mocks for wiki + chat: return the same object instance on every
// call so test assertions on `resetState` / `setWikiReady` observe the same
// mock fn the composable used internally.
const wikiResetStateMock = vi.fn();
const chatSetWikiReadyMock = vi.fn();
vi.mock('@/stores/chat', () => ({
  useChatStore: vi.fn(() => ({ setWikiReady: chatSetWikiReadyMock })),
}));
vi.mock('@/composables/use-summary', () => ({
  useSummary: vi.fn(() => ({ clearSummary: vi.fn() })),
}));
vi.mock('@/composables/use-wiki', () => ({
  useWiki: vi.fn(() => ({ resetState: wikiResetStateMock })),
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

    it('reloads the app on success so all cached view state is wiped', async () => {
      // After a successful import, all module-level singletons + keep-alive
      // caches must be cleared. The composable triggers a full reload.
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { importProject } = useExport();
      const file = new File(['{"project":"data"}'], 'backup.bango.json');
      await importProject(file);

      expect(reloadMock).toHaveBeenCalledTimes(1);
      // Lands on the Dashboard (fresh-start view) after the reload.
      expect(window.location.hash).toBe('#/');
    });

    it('sets error on import failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Invalid backup'));

      const { importProject, error } = useExport();
      const file = new File(['bad data'], 'backup.bango.json');
      await importProject(file);

      expect(error.value).toBe('Invalid backup');
      // No reload on failure - the user stays on the page to see the error.
      expect(reloadMock).not.toHaveBeenCalled();
    });
  });

  describe('exportRisForTab', () => {
    it('calls save dialog with tab-specific default path and sends flat args', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/error-articles.ris');
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { exportRisForTab } = useExport();
      const result = await exportRisForTab('error', true, 'Errors');

      expect(result).toBe(true);
      expect(save).toHaveBeenCalledWith({
        defaultPath: 'errors-articles.ris',
        filters: [{ name: 'RIS File', extensions: ['ris'] }],
      });
      // Flat args - no `request` wrapper
      expect(tauriCommand).toHaveBeenCalledWith('export_ris_for_tab_to_file', {
        path: '/path/to/error-articles.ris',
        status: 'error',
        screeningErrorsOnly: true,
      });
    });

    it('sends screeningErrorsOnly=false for non-error tabs', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/included-articles.ris');
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { exportRisForTab } = useExport();
      const result = await exportRisForTab('included', false, 'Included');

      expect(result).toBe(true);
      expect(tauriCommand).toHaveBeenCalledWith('export_ris_for_tab_to_file', {
        path: '/path/to/included-articles.ris',
        status: 'included',
        screeningErrorsOnly: false,
      });
    });

    it('slugifies the label for the default filename', async () => {
      vi.mocked(save).mockResolvedValue(null);

      const { exportRisForTab } = useExport();
      await exportRisForTab('all', false, 'All Articles');

      expect(save).toHaveBeenCalledWith(
        expect.objectContaining({
          defaultPath: 'all-articles-articles.ris',
        })
      );
    });

    it('returns false when user cancels save dialog', async () => {
      vi.mocked(save).mockResolvedValue(null);

      const { exportRisForTab } = useExport();
      const result = await exportRisForTab('rejected', false, 'Rejected');

      expect(result).toBe(false);
      expect(tauriCommand).not.toHaveBeenCalled();
    });

    it('sets error on tauri command failure', async () => {
      vi.mocked(save).mockResolvedValue('/path/to/working.ris');
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Tab export failed'));

      const { exportRisForTab, error, exporting } = useExport();
      const result = await exportRisForTab('working', false, 'Working');

      expect(result).toBe(false);
      expect(error.value).toBe('Tab export failed');
      expect(exporting.value).toBe(false);
    });

    it('handles non-Error exceptions', async () => {
      vi.mocked(save).mockResolvedValue('/path.ris');
      vi.mocked(tauriCommand).mockRejectedValue('unknown failure');

      const { exportRisForTab, error } = useExport();
      await exportRisForTab('duplicate', false, 'Duplicates');

      expect(error.value).toBe('unknown failure');
    });

    it('sets exporting=true during operation', async () => {
      let resolveCmd: () => void;
      const cmdPromise = new Promise<void>((r) => {
        resolveCmd = r;
      });
      vi.mocked(save).mockResolvedValue('/path.ris');
      vi.mocked(tauriCommand).mockReturnValue(cmdPromise);

      const { exportRisForTab, exporting } = useExport();
      const promise = exportRisForTab('error', true, 'Errors');

      expect(exporting.value).toBe(true);
      resolveCmd!();
      await promise;

      expect(exporting.value).toBe(false);
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

    it('resets wiki singleton and clears chat wiki readiness', async () => {
      // Delete All Data also wipes the on-disk wiki; the composable must reset
      // the wiki singleton state and the chat store's wikiReady flag.
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { resetProject } = useExport();
      await resetProject();

      expect(wikiResetStateMock).toHaveBeenCalledTimes(1);
      expect(chatSetWikiReadyMock).toHaveBeenCalledWith(false);
    });

    it('reloads the app on success so all cached view state is wiped', async () => {
      // After a successful reset, all module-level singletons + keep-alive
      // caches must be cleared. The composable triggers a full reload.
      vi.mocked(tauriCommand).mockResolvedValue(undefined);

      const { resetProject } = useExport();
      await resetProject();

      expect(reloadMock).toHaveBeenCalledTimes(1);
      // Lands on the Dashboard (fresh-start view) after the reload.
      expect(window.location.hash).toBe('#/');
    });

    it('sets error on reset failure', async () => {
      vi.mocked(tauriCommand).mockRejectedValue(new Error('Reset failed'));

      const { resetProject, error } = useExport();
      const result = await resetProject();

      expect(result).toBe(false);
      expect(error.value).toBe('Reset failed');
      // Wiki/chat reset is skipped when the backend reset fails.
      expect(wikiResetStateMock).not.toHaveBeenCalled();
      expect(chatSetWikiReadyMock).not.toHaveBeenCalled();
      // No reload on failure.
      expect(reloadMock).not.toHaveBeenCalled();
    });
  });
});
