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

describe('useScreening', () => {
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

  describe('startScreening', () => {
    it('sets loading and calls start_screening command', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockProgress);
      mockStore.readiness = { totalUnscreened: 100 };

      const { startScreening, loading, error } = useScreening();
      await startScreening();

      expect(tauriCommand).toHaveBeenCalledWith('start_screening', undefined);
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

    it('calls startListening immediately', async () => {
      vi.mocked(tauriCommand).mockResolvedValue(mockProgress);

      const { startScreening } = useScreening();
      await startScreening();

      expect(mockStore.startListening).toHaveBeenCalled();
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

    it('handles non-Error exceptions', async () => {
      vi.mocked(tauriCommand).mockRejectedValue('unexpected');
      mockStore.readiness = { totalUnscreened: 10 };

      const { startScreening, error } = useScreening();
      await startScreening();

      expect(error.value).toBe('unexpected');
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
});
