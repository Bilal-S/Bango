import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock tauri command
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

// Mock screening store with controllable behavior
const mockStore = {
  progress: null as Record<string, unknown> | null,
  readiness: null as Record<string, unknown> | null,
  loading: false,
  percentage: 0,
  estimatedTimeRemaining: null as string | null,
  fetchReadiness: vi.fn().mockResolvedValue(undefined),
  refreshProgress: vi.fn().mockResolvedValue(undefined),
  setProgress: vi.fn(),
  startListening: vi.fn().mockResolvedValue(undefined),
  stopListening: vi.fn(),
  resetScreeningErrors: vi.fn().mockResolvedValue(0),
  resetWorkingList: vi.fn().mockResolvedValue(0),
};

vi.mock('@/stores/screening', () => ({
  useScreeningStore: vi.fn(() => mockStore),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useScreening } from '@/composables/use-screening';

const mockProgress = {
  total: 100,
  completed: 50,
  included: 30,
  rejected: 15,
  errors: 5,
  isRunning: false,
  currentArticleTitles: [],
  elapsedMs: 60000,
  estimatedRemainingMs: 60000,
};

/* Refactor1 T5.3: the suite sections below were nested inside one giant
 * describe callback (useScreening, 670 lines). They are now top-level describes sharing
 * the module-scope hooks; the redundant suite-name prefix is dropped. */

beforeEach(() => {
  vi.clearAllMocks();
  mockStore.progress = null;
  mockStore.readiness = null;
  mockStore.loading = false;
  mockStore.percentage = 0;
  mockStore.estimatedTimeRemaining = null;
});

describe('initial state', () => {
  it('is not loading', () => {
    const { loading } = useScreening();
    expect(loading.value).toBe(false);
  });

  it('has no error', () => {
    const { error } = useScreening();
    expect(error.value).toBeNull();
  });

  it('exposes store progress', () => {
    mockStore.progress = mockProgress;
    const { progress } = useScreening();
    expect(progress.value).toEqual(mockProgress);
  });

  it('exposes store readiness', () => {
    mockStore.readiness = { totalUnscreened: 50, hasCriteria: true, hasProvider: true };
    const { readiness } = useScreening();
    expect(readiness.value).toEqual({
      totalUnscreened: 50,
      hasCriteria: true,
      hasProvider: true,
    });
  });
});

describe('fetchReadiness', () => {
  it('delegates to store fetchReadiness', async () => {
    const { fetchReadiness } = useScreening();
    await fetchReadiness();
    expect(mockStore.fetchReadiness).toHaveBeenCalled();
  });
});

/* Refactor1 T5.3: the 511-line startScreening describe is split into three
 * thematic suites (command payload, optimistic progress, lifecycle/errors).
 * The it-blocks are unchanged; only their grouping moved. */
describe('startScreening - command payload', () => {
  it('sets loading and calls start_screening command', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening, loading, error } = useScreening();
    await startScreening();

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {});
    expect(loading.value).toBe(false);
    expect(error.value).toBeNull();
  });

  it('passes batchSize when provided', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(5);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', { batchSize: 5 });
  });

  it('passes maxArticles when provided', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(3, 12);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {
      batchSize: 3,
      maxArticles: 12,
    });
  });

  it('passes only maxArticles when batchSize is omitted', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(undefined, 9);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', { maxArticles: 9 });
  });

  it('passes empty args when both batchSize and maxArticles are omitted', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(undefined, undefined);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {});
  });

  it('accepts maxArticles without readiness and still sends command args', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = null;

    const { startScreening } = useScreening();
    await startScreening(1, 6);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {
      batchSize: 1,
      maxArticles: 6,
    });
  });

  it('keeps legacy batch-only command shape unchanged', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(5);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', { batchSize: 5 });
  });

  it('still supports no-args start command', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening();

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {});
  });

  it('handles undefined readiness with no args start', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = null;

    const { startScreening } = useScreening();
    await startScreening();

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 0
    );
    expect(optimisticCall).toBeDefined();
  });

  it('keeps tauri command payload deterministic for explicit undefined values', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(undefined, undefined);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {});
  });

  it('keeps tauri command payload deterministic for only batchSize defined', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(6, undefined);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', { batchSize: 6 });
  });

  it('keeps tauri command payload deterministic for only maxArticles defined', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(undefined, 6);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', { maxArticles: 6 });
  });

  it('retains backward compatibility for old callers passing only batch size', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(4);

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', { batchSize: 4 });
  });

  it('uses empty payload for legacy no-arg callers', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening();

    expect(tauriCommand).toHaveBeenCalledWith('start_screening', {});
  });
});

describe('startScreening - optimistic progress', () => {
  it('sets optimistic total to maxArticles cap when provided', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 7);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 7
    );
    expect(optimisticCall).toBeDefined();
  });

  it('clamps optimistic total by unscreened count', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 5 };

    const { startScreening } = useScreening();
    await startScreening(1, 99);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 5
    );
    expect(optimisticCall).toBeDefined();
  });

  it('clamps optimistic total minimum to 1 when maxArticles is 0', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 0);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 1
    );
    expect(optimisticCall).toBeDefined();
  });

  it('sets optimistic total to 0 when no unscreened articles are available', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 0 };

    const { startScreening } = useScreening();
    await startScreening(1, 5);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 0
    );
    expect(optimisticCall).toBeDefined();
  });

  it('sets optimistic total to unscreened when maxArticles is omitted', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 42 };

    const { startScreening } = useScreening();
    await startScreening(2);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 42
    );
    expect(optimisticCall).toBeDefined();
  });

  it('replaces optimistic progress with real result when total > 0 after maxArticles start', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ ...mockProgress, total: 12, completed: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(2, 12);

    const realCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.total === 12 && c[0]?.completed === 0
    );
    expect(realCall).toBeDefined();
  });

  it('keeps existing optimistic progress test for baseline path', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening();

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 100
    );
    expect(optimisticCall).toBeDefined();
  });

  it('clears optimistic progress on error when maxArticles is used', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Screening failed'));
    mockStore.readiness = { totalUnscreened: 50 };

    const { startScreening, error } = useScreening();
    await startScreening(2, 8);

    expect(error.value).toBe('Screening failed');
    expect(mockStore.setProgress).toHaveBeenCalledWith(null);
  });

  it('does not set real progress when backend returns total 0', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 5);

    const realCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) =>
        c[0]?.total === 0 && c[0]?.completed === 0 && c[0]?.isRunning === undefined
    );
    expect(realCall).toBeUndefined();
  });

  it('ensures optimistic isRunning true when maxArticles provided', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(2, 11);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true
    );
    expect(optimisticCall).toBeDefined();
  });

  it('sets optimistic completed/included/rejected/errors to 0 with maxArticles', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 5);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) =>
        c[0]?.isRunning === true &&
        c[0]?.completed === 0 &&
        c[0]?.included === 0 &&
        c[0]?.rejected === 0 &&
        c[0]?.errors === 0
    );
    expect(optimisticCall).toBeDefined();
  });

  it('keeps currentArticleTitles empty in optimistic state when maxArticles set', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 5);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) =>
        c[0]?.isRunning === true && Array.isArray(c[0]?.currentArticleTitles)
    );
    expect(optimisticCall).toBeDefined();
  });

  it('ensures optimistic elapsed timing fields are initialized when maxArticles set', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 5);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) =>
        c[0]?.isRunning === true && c[0]?.elapsedMs === 0 && c[0]?.estimatedRemainingMs === null
    );
    expect(optimisticCall).toBeDefined();
  });

  it('supports maxArticles equal to totalUnscreened exactly', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 12 };

    const { startScreening } = useScreening();
    await startScreening(1, 12);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 12
    );
    expect(optimisticCall).toBeDefined();
  });

  it('supports tiny maxArticles value of 1', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 12 };

    const { startScreening } = useScreening();
    await startScreening(1, 1);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 1
    );
    expect(optimisticCall).toBeDefined();
  });

  it('still sets optimistic state before awaiting IPC', async () => {
    let resolveFn!: (v: Record<string, unknown>) => void;
    const pending = new Promise<Record<string, unknown>>((resolve) => {
      resolveFn = resolve;
    });
    vi.mocked(tauriCommand).mockReturnValue(pending as unknown as ReturnType<typeof tauriCommand>);
    mockStore.readiness = { totalUnscreened: 10 };

    const { startScreening } = useScreening();
    const promise = startScreening(1, 3);

    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 3
    );
    expect(optimisticCall).toBeDefined();

    resolveFn({ total: 0 });
    await promise;
  });

  it('sets optimistic progress immediately', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening();

    // Should call setProgress with optimistic data
    expect(mockStore.setProgress).toHaveBeenCalled();
    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 100
    );
    expect(optimisticCall).toBeDefined();
  });

  it('sets optimistic progress immediately', async () => {
    vi.mocked(tauriCommand).mockResolvedValue({ total: 0 });
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening();

    // Should call setProgress with optimistic data
    expect(mockStore.setProgress).toHaveBeenCalled();
    const optimisticCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.isRunning === true && c[0]?.total === 100
    );
    expect(optimisticCall).toBeDefined();
  });

  it('sets error on failure and clears optimistic progress', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Screening failed'));
    mockStore.readiness = { totalUnscreened: 50 };

    const { startScreening, error, loading } = useScreening();
    await startScreening();

    expect(error.value).toBe('Screening failed');
    expect(loading.value).toBe(false);
    // Should clear progress on error (setProgress called with null)
    expect(mockStore.setProgress).toHaveBeenCalledWith(null);
  });

  it('replaces optimistic progress with real result when total > 0', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening();

    // Should have called setProgress with real data (total=100)
    const realCall = mockStore.setProgress.mock.calls.find(
      (c: Array<Record<string, unknown>>) => c[0]?.total === 100 && c[0]?.completed === 50
    );
    expect(realCall).toBeDefined();
  });
});

describe('startScreening - lifecycle, loading, and errors', () => {
  it('preserves loading and error behavior with maxArticles', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 20 };

    const { startScreening, loading, error } = useScreening();
    await startScreening(1, 10);

    expect(loading.value).toBe(false);
    expect(error.value).toBeNull();
  });

  it('starts listening immediately when maxArticles is used', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening } = useScreening();
    await startScreening(1, 4);

    expect(mockStore.startListening).toHaveBeenCalled();
  });

  it('handles non-Error exceptions with maxArticles', async () => {
    vi.mocked(tauriCommand).mockRejectedValue('unexpected');
    mockStore.readiness = { totalUnscreened: 10 };

    const { startScreening, error } = useScreening();
    await startScreening(1, 3);

    expect(error.value).toBe('unexpected');
  });

  it('keeps loading true during pending start and false after completion', async () => {
    let resolveFn!: (v: Record<string, unknown>) => void;
    const pending = new Promise<Record<string, unknown>>((resolve) => {
      resolveFn = resolve;
    });
    vi.mocked(tauriCommand).mockReturnValue(pending as unknown as ReturnType<typeof tauriCommand>);
    mockStore.readiness = { totalUnscreened: 10 };

    const { startScreening, loading } = useScreening();
    const promise = startScreening(1, 3);
    expect(loading.value).toBe(true);

    resolveFn({ total: 0 });
    await promise;
    expect(loading.value).toBe(false);
  });

  it('keeps error null on success path with maxArticles', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
    mockStore.readiness = { totalUnscreened: 100 };

    const { startScreening, error } = useScreening();
    await startScreening(1, 8);

    expect(error.value).toBeNull();
  });

  it('calls startListening immediately', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);

    const { startScreening } = useScreening();
    await startScreening();

    expect(mockStore.startListening).toHaveBeenCalled();
  });

  it('handles non-Error exceptions', async () => {
    vi.mocked(tauriCommand).mockRejectedValue('unexpected');
    mockStore.readiness = { totalUnscreened: 10 };

    const { startScreening, error } = useScreening();
    await startScreening();

    expect(error.value).toBe('unexpected');
  });
});

describe('pauseScreening', () => {
  it('calls pause_screening command', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(undefined);

    const { pauseScreening } = useScreening();
    await pauseScreening();

    expect(tauriCommand).toHaveBeenCalledWith('pause_screening');
  });

  it('ignores errors silently', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Not running'));

    const { pauseScreening, error } = useScreening();
    await pauseScreening();

    expect(error.value).toBeNull();
  });
});

describe('resumeScreening', () => {
  it('calls resume_screening command', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(undefined);

    const { resumeScreening } = useScreening();
    await resumeScreening();

    expect(tauriCommand).toHaveBeenCalledWith('resume_screening');
  });

  it('ignores errors silently', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Not paused'));

    const { resumeScreening, error } = useScreening();
    await resumeScreening();

    expect(error.value).toBeNull();
  });
});

describe('stopScreening', () => {
  it('calls stop_screening command', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(undefined);

    const { stopScreening } = useScreening();
    await stopScreening();

    expect(tauriCommand).toHaveBeenCalledWith('stop_screening');
  });

  it('ignores errors silently', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Already stopped'));

    const { stopScreening, error } = useScreening();
    await stopScreening();

    expect(error.value).toBeNull();
  });
});

describe('resetScreeningErrors', () => {
  it('delegates to store', async () => {
    mockStore.resetScreeningErrors.mockResolvedValue(3);

    const { resetScreeningErrors } = useScreening();
    const count = await resetScreeningErrors();

    expect(count).toBe(3);
    expect(mockStore.resetScreeningErrors).toHaveBeenCalled();
  });
});

describe('resetWorkingList', () => {
  it('delegates to store', async () => {
    mockStore.resetWorkingList.mockResolvedValue(5);

    const { resetWorkingList } = useScreening();
    const count = await resetWorkingList();

    expect(count).toBe(5);
    expect(mockStore.resetWorkingList).toHaveBeenCalled();
  });
});

describe('startListening / stopListening', () => {
  it('delegates startListening to store', async () => {
    const { startListening } = useScreening();
    await startListening();
    expect(mockStore.startListening).toHaveBeenCalled();
  });

  it('delegates stopListening to store', () => {
    const { stopListening } = useScreening();
    stopListening();
    expect(mockStore.stopListening).toHaveBeenCalled();
  });
});

describe('tokenWarning', () => {
  it('returns null when no readiness', () => {
    mockStore.readiness = null;
    const { tokenWarning } = useScreening();
    expect(tokenWarning.value).toBeNull();
  });

  it('returns tokenWarning from readiness', () => {
    mockStore.readiness = { tokenWarning: 'Too many tokens' };
    const { tokenWarning } = useScreening();
    expect(tokenWarning.value).toBe('Too many tokens');
  });
});
