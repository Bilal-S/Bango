import { ref, computed } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface ScreeningProgress {
  total: number;
  completed: number;
  included: number;
  rejected: number;
  errors: number;
  isRunning: boolean;
  currentArticleTitle: string | null;
  elapsedMs: number;
  estimatedRemainingMs: number | null;
}

export function useScreening() {
  const progress = ref<ScreeningProgress | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const tokenWarning = ref<string | null>(null);

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

  async function checkTokenEstimate(): Promise<void> {
    try {
      tokenWarning.value = await tauriCommand<string | null>('estimate_screening_tokens');
    } catch {
      // Ignore -- may not have config yet
    }
  }

  async function startScreening(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      progress.value = await tauriCommand<ScreeningProgress>('start_screening');
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
    error,
    tokenWarning,
    percentage,
    estimatedTimeRemaining,
    startScreening,
    refreshProgress,
    checkTokenEstimate,
    pauseScreening,
    resumeScreening,
    stopScreening,
  };
}
