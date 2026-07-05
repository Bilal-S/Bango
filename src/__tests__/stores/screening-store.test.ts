import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useScreeningStore } from '@/stores/screening';
import type { ScreeningReadiness, ScreeningProgress } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

// Prevent dynamic import of @tauri-apps/api/event in startListening.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockReadiness: ScreeningReadiness = {
  totalWorking: 90,
  totalUnscreened: 10,
  hasAims: true,
  hasInclusion: true,
  hasExclusion: true,
  hasLlmConfig: true,
  tokenWarning: null,
  progress: null,
};

const mockProgress: ScreeningProgress = {
  total: 10,
  completed: 5,
  included: 4,
  rejected: 1,
  errors: 0,
  isRunning: true,
  currentArticleTitles: ['Article 1', 'Article 2'],
  elapsedMs: 30000,
  estimatedRemainingMs: 60000,
};

describe('useScreeningStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty', () => {
    const store = useScreeningStore();
    expect(store.readiness).toBeNull();
    expect(store.progress).toBeNull();
    expect(store.initialized).toBe(false);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it('percentage is zero when no progress', () => {
    const store = useScreeningStore();
    expect(store.percentage).toBe(0);
  });

  it('percentage is computed from completed/total', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 3,
      included: 2,
      rejected: 1,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 10000,
      estimatedRemainingMs: 0,
    });
    expect(store.percentage).toBe(30);
  });

  it('estimatedTimeRemaining formats seconds', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 5,
      included: 4,
      rejected: 1,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 20000,
      estimatedRemainingMs: 45000,
    });
    expect(store.estimatedTimeRemaining).toBe('45s');
  });

  it('estimatedTimeRemaining formats minutes', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 5,
      included: 4,
      rejected: 1,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 25000,
      estimatedRemainingMs: 125000,
    });
    expect(store.estimatedTimeRemaining).toBe('2m 5s');
  });

  it('estimatedTimeRemaining returns dash when no progress', () => {
    const store = useScreeningStore();
    expect(store.estimatedTimeRemaining).toBe('-');
  });

  it('fetchReadiness populates state', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockReadiness);

    const store = useScreeningStore();
    await store.fetchReadiness();

    expect(store.readiness).toEqual(mockReadiness);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
  });

  it('fetchReadiness handles errors', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('screening error'));

    const store = useScreeningStore();
    await store.fetchReadiness();

    expect(store.error).toBe('screening error');
    expect(store.initialized).toBe(false);
  });

  it('refreshProgress updates progress', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockProgress);

    const store = useScreeningStore();
    await store.refreshProgress();

    expect(store.progress).toEqual(mockProgress);
  });

  it('refreshProgress swallows errors', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('network'));

    const store = useScreeningStore();
    await store.refreshProgress();

    // No throw; progress remains null.
    expect(store.progress).toBeNull();
  });

  it('invalidate clears all state', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockReadiness);

    const store = useScreeningStore();
    await store.fetchReadiness();
    expect(store.initialized).toBe(true);

    store.invalidate();
    expect(store.initialized).toBe(false);
    expect(store.readiness).toBeNull();
    expect(store.progress).toBeNull();
  });

  it('setProgress overwrites progress', () => {
    const store = useScreeningStore();
    store.setProgress(mockProgress);
    expect(store.progress).toEqual(mockProgress);

    store.setProgress(null);
    expect(store.progress).toBeNull();
  });

  it('resetScreeningErrors calls command and refreshes', async () => {
    vi.mocked(tauriCommand).mockResolvedValueOnce(3); // reset returns count
    vi.mocked(tauriCommand).mockResolvedValueOnce(mockReadiness); // re-fetch

    const store = useScreeningStore();
    const count = await store.resetScreeningErrors();

    expect(count).toBe(3);
    expect(tauriCommand).toHaveBeenCalledWith('reset_screening_errors');
    expect(tauriCommand).toHaveBeenCalledWith('get_screening_readiness');
  });

  it('resetWorkingList calls command and refreshes', async () => {
    vi.mocked(tauriCommand).mockResolvedValueOnce(5);
    vi.mocked(tauriCommand).mockResolvedValueOnce(mockReadiness);

    const store = useScreeningStore();
    const count = await store.resetWorkingList();

    expect(count).toBe(5);
    expect(tauriCommand).toHaveBeenCalledWith('reset_working_list');
    expect(tauriCommand).toHaveBeenCalledWith('get_screening_readiness');
  });
});
