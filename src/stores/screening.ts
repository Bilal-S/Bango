import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { ScreeningProgress, ScreeningReadiness } from '@/types';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

export const useScreeningStore = defineStore('screening', () => {
  const readiness = ref<ScreeningReadiness | null>(null);
  const progress = ref<ScreeningProgress | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const initialized = ref(false);

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
    loading.value = true;
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
      loading.value = false;
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
  };
});
