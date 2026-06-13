import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export type CitationStyle = 'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA';

interface SavedSummary {
  summaryText: string;
  citationStyle: string;
  generatedAt: string;
}

/* ── Module-level singleton state (shared across all callers) ── */
const summaryText = ref<string | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const generatedAt = ref<string | null>(null);
const citationStyle = ref<CitationStyle>('APA');

export function useSummary() {
  async function loadSaved(): Promise<void> {
    try {
      const saved = await tauriCommand<SavedSummary | null>('get_saved_summary', {});
      if (saved) {
        summaryText.value = saved.summaryText;
        generatedAt.value = saved.generatedAt;
        if (saved.citationStyle) {
          citationStyle.value = saved.citationStyle as CitationStyle;
        }
      }
    } catch {
      // Silently ignore - saved summary is optional
    }
  }

  /** Reset all summary state (called on import or project reset) */
  function clearSummary(): void {
    summaryText.value = null;
    generatedAt.value = null;
    citationStyle.value = 'APA';
    error.value = null;
  }

  async function generate(style: CitationStyle = 'APA'): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const result = await tauriCommand<string>('generate_summary', {
        citationStyle: style,
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

  return {
    summaryText,
    loading,
    error,
    generatedAt,
    citationStyle,
    loadSaved,
    generate,
    clearSummary,
    formatGeneratedAt,
  };
}
