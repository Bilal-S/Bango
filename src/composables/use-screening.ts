import { ref, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';
import { useScreeningStore } from '@/stores/screening';
import type { ScreeningProgress } from '@/types';

export function useScreening() {
  const store = useScreeningStore();
  const loading = ref(false);
  const error = ref<string | null>(null);

  const progress = computed(() => store.progress);
  const readiness = computed(() => store.readiness);
  const readinessLoading = computed(() => store.loading);
  const percentage = computed(() => store.percentage);
  const estimatedTimeRemaining = computed(() => store.estimatedTimeRemaining);
  const tokenWarning = computed(() => store.readiness?.tokenWarning ?? null);

  async function fetchReadiness(): Promise<void> {
    await store.fetchReadiness();
  }

  async function startScreening(batchSize?: number, maxArticles?: number): Promise<void> {
    loading.value = true;
    error.value = null;

    // Optimistically show progress bar immediately (before IPC returns).
    // If a max-articles cap is provided, reflect that cap in the optimistic total.
    const totalUnscreened = store.readiness?.totalUnscreened ?? 0;
    const optimisticTotal =
      maxArticles !== undefined
        ? Math.min(Math.max(maxArticles, 1), totalUnscreened)
        : totalUnscreened;

    store.setProgress({
      total: optimisticTotal,
      completed: 0,
      included: 0,
      rejected: 0,
      errors: 0,
      isRunning: true,
      currentArticleTitles: [],
      elapsedMs: 0,
      estimatedRemainingMs: null,
    });

    // Start listening for live progress events immediately
    store.startListening();

    try {
      const args: Record<string, unknown> = {};
      if (batchSize !== undefined) args.batchSize = batchSize;
      if (maxArticles !== undefined) args.maxArticles = maxArticles;
      const result = await tauriCommand<ScreeningProgress>('start_screening', args);
      // Replace optimistic progress with real initial progress (may have total=0 if engine
      // hasn't counted yet - that's fine, the next event will correct it)
      if (result.total > 0) {
        store.setProgress(result);
      }
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      store.setProgress(null); // Clear optimistic progress on error
    } finally {
      loading.value = false;
    }
  }

  async function refreshProgress(): Promise<void> {
    await store.refreshProgress();
  }

  async function pauseScreening(): Promise<void> {
    try {
      await tauriCommand('pause_screening');
    } catch {
      // Ignore
    }
  }

  async function resumeScreening(): Promise<void> {
    try {
      await tauriCommand('resume_screening');
    } catch {
      // Ignore
    }
  }

  async function stopScreening(): Promise<void> {
    try {
      await tauriCommand('stop_screening');
    } catch {
      // Ignore
    }
  }

  async function startListening(): Promise<void> {
    await store.startListening();
  }

  function stopListening(): void {
    store.stopListening();
  }

  async function resetScreeningErrors(): Promise<number> {
    return await store.resetScreeningErrors();
  }

  async function resetWorkingList(): Promise<number> {
    return await store.resetWorkingList();
  }

  return {
    progress,
    loading,
    readinessLoading,
    error,
    tokenWarning,
    readiness,
    percentage,
    estimatedTimeRemaining,
    fetchReadiness,
    startScreening,
    refreshProgress,
    pauseScreening,
    resumeScreening,
    stopScreening,
    startListening,
    stopListening,
    resetScreeningErrors,
    resetWorkingList,
  };
}
