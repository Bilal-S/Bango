import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface SummaryOutput {
  keyThemes: string;
  researchTrends: string;
  methodologicalStrengths: string;
  commonWeaknesses: string;
  gapsInLiterature: string;
}

export function useSummary() {
  const summary = ref<SummaryOutput | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function generate(targetLength = 1000): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      summary.value = await tauriCommand<SummaryOutput>('generate_summary', { targetLength });
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return { summary, loading, error, generate };
}
