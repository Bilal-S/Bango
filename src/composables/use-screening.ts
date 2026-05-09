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

  async function startScreening(batchSize?: number): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const args = batchSize ? { batchSize } : undefined;
      const result = await tauriCommand<ScreeningProgress>('start_screening', args);
      store.setProgress(result);
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
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
  };
}
