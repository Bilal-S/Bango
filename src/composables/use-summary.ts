import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export type CitationStyle = 'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA';

export interface SavedSummary {
  summaryText: string;
  citationStyle: string;
  generatedAt: string;
}

export function useSummary() {
  const summaryText = ref<string | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const generatedAt = ref<string | null>(null);

  async function loadSaved(): Promise<void> {
    try {
      const saved = await tauriCommand<SavedSummary | null>('get_saved_summary', {});
      if (saved) {
        summaryText.value = saved.summaryText;
        generatedAt.value = saved.generatedAt;
      }
    } catch {
      // Silently ignore - saved summary is optional
    }
  }

  async function generate(citationStyle: CitationStyle = 'APA'): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const result = await tauriCommand<string>('generate_summary', {
        citationStyle,
      });
      summaryText.value = result;
      // The backend saves with timestamp; reload to get the exact server timestamp
      await loadSaved();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  /** Format the ISO timestamp using the user's system locale */
  function formatGeneratedAt(): string | null {
    if (!generatedAt.value) return null;
    try {
      const date = new Date(generatedAt.value);
      return new Intl.DateTimeFormat(undefined, {
        dateStyle: 'medium',
        timeStyle: 'short',
      }).format(date);
    } catch {
      return generatedAt.value;
    }
  }

  return { summaryText, loading, error, generatedAt, loadSaved, generate, formatGeneratedAt };
}
