import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('vue', async (importOriginal) => {
  const mod = await importOriginal<typeof import('vue')>();
  return {
    ...mod,
    onMounted: vi.fn(),
  };
});

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { useBibliometrics } from '@/composables/use-bibliometrics';
import { tauriCommand } from '@/composables/use-tauri-command';

// Mock the Tauri event listen function
const mockListen = vi.fn();
vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, callback: unknown) => {
    mockListen(event, callback);
    return Promise.resolve(() => {});
  },
}));

describe('useBibliometrics', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should initialize with default values', () => {
    const { kpis, normalizing, progress, error } = useBibliometrics();
    expect(kpis.value.includedCount).toBe(0);
    expect(normalizing.value).toBe(false);
    expect(progress.value).toBe(0);
    expect(error.value).toBeNull();
  });

  it('runs normalization, listens to events, and updates progress', async () => {
    const testState = {
      progressCallback: null as
        | ((event: { payload: { step: number; totalSteps: number; message: string } }) => void)
        | null,
      resolveTauriCommand: null as ((res: unknown) => void) | null,
    };

    mockListen.mockImplementation(
      (
        event: string,
        cb: (event: { payload: { step: number; totalSteps: number; message: string } }) => void
      ) => {
        if (event === 'biblio:progress') {
          testState.progressCallback = cb;
        }
        return Promise.resolve(() => {});
      }
    );

    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'biblio_normalize') {
        return new Promise((r) => {
          testState.resolveTauriCommand = r;
        });
      }
      if (cmd === 'biblio_get_kpis') {
        return Promise.resolve({ includedCount: 5 });
      }
      return Promise.resolve({});
    });

    const { normalizing, progress, runNormalization } = useBibliometrics();

    // Call runNormalization
    const normPromise = runNormalization();

    // Wait slightly to allow the setup (listen and macro-tasks) to run
    await new Promise<void>((r) => setTimeout(r, 20));

    expect(normalizing.value).toBe(true);

    // Manually trigger the progress event callback
    if (testState.progressCallback) {
      testState.progressCallback({ payload: { step: 3, totalSteps: 6, message: 'Step 3' } });
    }
    // Step 3 progress = 3 * (100 / 6) = 50%
    expect(progress.value).toBe(50);

    // Complete the normalization command
    if (testState.resolveTauriCommand) {
      testState.resolveTauriCommand({ authors: 10, terms: 5, status: {} });
    }

    // Wait for the normalization to fully complete (including the 500ms delay)
    await normPromise;

    // After completion, progress should be 100
    expect(progress.value).toBe(100);
    expect(normalizing.value).toBe(false);
  });
});
