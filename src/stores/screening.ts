import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { ScreeningProgress, ScreeningReadiness } from '@/types';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';
import { useArticlesStore } from '@/stores/articles';
import { useAuditStore } from '@/stores/audit';

type UnlistenFn = () => void;

export const useScreeningStore = defineStore('screening', () => {
  const readiness = ref<ScreeningReadiness | null>(null);
  const progress = ref<ScreeningProgress | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const initialized = ref(false);
  let unlistenProgress: UnlistenFn | null = null;

  const percentage = computed(() => {
    if (!progress.value || progress.value.total === 0) return 0;
    return Math.round((progress.value.completed / progress.value.total) * 100);
  });

  const estimatedTimeRemaining = computed((): string => {
    if (!progress.value?.estimatedRemainingMs) return '-';
    const seconds = Math.ceil(progress.value.estimatedRemainingMs / 1000);
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}m ${remainingSeconds}s`;
  });

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetchReadiness();
  }

  async function fetchReadiness(): Promise<void> {
    const isFirstLoad = !initialized.value;
    if (isFirstLoad) {
      loading.value = true;
    }
    error.value = null;
    try {
      const data = await tauriCommand<ScreeningReadiness>('get_screening_readiness');
      readiness.value = data;
      if (data.progress) {
        progress.value = data.progress;
      }
      initialized.value = true;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (isFirstLoad) {
        loading.value = false;
      }
    }
  }

  async function refreshProgress(): Promise<void> {
    try {
      progress.value = await tauriCommand<ScreeningProgress>('get_screening_progress');
    } catch {
      // Ignore
    }
  }

  function setProgress(newProgress: ScreeningProgress | null): void {
    progress.value = newProgress;
  }

  /** Start listening for `screening:progress` events from the backend engine. */
  async function startListening(): Promise<void> {
    if (unlistenProgress || !isTauri()) return;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      unlistenProgress = await listen<ScreeningProgress>('screening:progress', (event) => {
        progress.value = event.payload;
        if (!event.payload.isRunning) {
          stopListening();
          // Refresh readiness after run completes
          void fetchReadiness();
          // Invalidate articles + audit stores so dashboard summary refreshes
          const articlesStore = useArticlesStore();
          const auditStore = useAuditStore();
          articlesStore.invalidate();
          auditStore.invalidate();
          void Promise.all([articlesStore.fetchIfNeeded(), auditStore.fetchIfNeeded()]);
        }
      });
    } catch {
      // Tauri event system unavailable — fall back gracefully
    }
  }

  /** Stop listening for progress events. */
  function stopListening(): void {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
  }

  function invalidate(): void {
    readiness.value = null;
    progress.value = null;
    initialized.value = false;
  }

  /** Reset screening errors so errored articles can be re-screened. */
  async function resetScreeningErrors(): Promise<number> {
    const count = await tauriCommand<number>('reset_screening_errors');
    // Refresh readiness to reflect the new unscreened count
    await fetchReadiness();
    return count;
  }

  /** Reset the working list: clear screened_at for all working articles so they can be re-screened. */
  async function resetWorkingList(): Promise<number> {
    const count = await tauriCommand<number>('reset_working_list');
    await fetchReadiness();
    return count;
  }

  return {
    readiness,
    progress,
    loading,
    error,
    initialized,
    percentage,
    estimatedTimeRemaining,
    fetchIfNeeded,
    fetchReadiness,
    refreshProgress,
    setProgress,
    startListening,
    stopListening,
    invalidate,
    resetScreeningErrors,
    resetWorkingList,
  };
});
