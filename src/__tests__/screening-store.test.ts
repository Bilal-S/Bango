import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useScreeningStore } from '@/stores/screening';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

describe('useScreeningStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('starts with null progress and readiness', () => {
    const store = useScreeningStore();
    expect(store.progress).toBeNull();
    expect(store.readiness).toBeNull();
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
    expect(store.initialized).toBe(false);
  });

  it('percentage is 0 when no progress', () => {
    const store = useScreeningStore();
    expect(store.percentage).toBe(0);
  });

  it('percentage is 0 when total is 0', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 0,
      completed: 0,
      included: 0,
      rejected: 0,
      errors: 0,
      isRunning: false,
      currentArticleTitles: [],
      elapsedMs: 0,
      estimatedRemainingMs: null,
    });
    expect(store.percentage).toBe(0);
  });

  it('percentage computes correctly at 50%', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 5,
      included: 3,
      rejected: 2,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 1000,
      estimatedRemainingMs: 1000,
    });
    expect(store.percentage).toBe(50);
  });

  it('percentage computes correctly at 100%', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 10,
      included: 6,
      rejected: 4,
      errors: 0,
      isRunning: false,
      currentArticleTitles: [],
      elapsedMs: 5000,
      estimatedRemainingMs: null,
    });
    expect(store.percentage).toBe(100);
  });

  it('percentage rounds to nearest integer', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 3,
      completed: 1,
      included: 1,
      rejected: 0,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 500,
      estimatedRemainingMs: 1000,
    });
    // 1/3 = 33.33... → rounds to 33
    expect(store.percentage).toBe(33);
  });

  it('estimatedTimeRemaining shows seconds for <60s', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 5,
      included: 3,
      rejected: 2,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 5000,
      estimatedRemainingMs: 30000,
    });
    expect(store.estimatedTimeRemaining).toBe('30s');
  });

  it('estimatedTimeRemaining shows minutes and seconds for >=60s', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 2,
      included: 1,
      rejected: 1,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 2000,
      estimatedRemainingMs: 125000,
    });
    // 125s = 2m 5s
    expect(store.estimatedTimeRemaining).toBe('2m 5s');
  });

  it('estimatedTimeRemaining shows dash when null', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 5,
      included: 3,
      rejected: 2,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 1000,
      estimatedRemainingMs: null,
    });
    expect(store.estimatedTimeRemaining).toBe('-');
  });

  it('invalidate resets state', () => {
    const store = useScreeningStore();
    store.setProgress({
      total: 10,
      completed: 5,
      included: 3,
      rejected: 2,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 1000,
      estimatedRemainingMs: 500,
    });
    store.initialized = true;
    store.invalidate();
    expect(store.progress).toBeNull();
    expect(store.readiness).toBeNull();
    expect(store.initialized).toBe(false);
  });

  it('fetchIfNeeded does nothing in non-Tauri mode', async () => {
    const store = useScreeningStore();
    await store.fetchIfNeeded();
    expect(store.initialized).toBe(false);
  });

  it('setProgress updates progress', () => {
    const store = useScreeningStore();
    const p = {
      total: 5,
      completed: 1,
      included: 0,
      rejected: 1,
      errors: 0,
      isRunning: true,
      currentArticleTitles: ['Article 1'],
      elapsedMs: 500,
      estimatedRemainingMs: 2000,
    };
    store.setProgress(p);
    expect(store.progress).toEqual(p);
    expect(store.progress!.currentArticleTitles).toContain('Article 1');
  });
});
