import { describe, it, expect, vi, beforeEach } from 'vitest';

/* Characterization tests for the RIS-export scaffold that Tier 1 (T1.3) of
 * .worktrees/refactor1.md extracts into a shared helper. Pins the three
 * behavioral contracts: dialog cancel, tab-export IPC payload, and error
 * propagation. See also src/__tests__/export-lifecycle.test.ts for the full
 * composable suite. */

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useExport } from '@/composables/use-export';

describe('useExport - RIS export scaffold (refactor1 Tier 0)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriCommand).mockResolvedValue(undefined);
  });

  it('export_ris_returns_false_when_dialog_cancelled', async () => {
    vi.mocked(save).mockResolvedValue(null);

    const { exportRis, exporting, error } = useExport();
    const result = await exportRis();

    expect(result).toBe(false);
    expect(tauriCommand).not.toHaveBeenCalled();
    expect(exporting.value).toBe(false);
    expect(error.value).toBeNull();
  });

  it('export_ris_for_tab_passes_status_and_errors_flag', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/included-articles.ris');

    const { exportRisForTab } = useExport();
    const result = await exportRisForTab('included', true, 'Included');

    expect(result).toBe(true);
    /* Default filename is derived from the tab label (slugified). */
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: 'included-articles.ris' })
    );
    expect(tauriCommand).toHaveBeenCalledWith('export_ris_for_tab_to_file', {
      path: '/tmp/included-articles.ris',
      status: 'included',
      screeningErrorsOnly: true,
    });
  });

  it('export_ris_reports_invoke_error', async () => {
    vi.mocked(save).mockResolvedValue('/tmp/included-articles.ris');
    vi.mocked(tauriCommand).mockRejectedValue(new Error('RIS export failed'));

    const { exportRis, error, exporting } = useExport();
    const result = await exportRis();

    expect(result).toBe(false);
    expect(error.value).toBe('RIS export failed');
    expect(exporting.value).toBe(false);
  });
});
