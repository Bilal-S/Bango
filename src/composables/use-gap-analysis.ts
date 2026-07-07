import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export type CitationStyle = 'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA';

interface SavedGapAnalysis {
  gapText: string;
  citationStyle: string;
  generatedAt: string;
}

/* ── Module-level singleton state (shared across all callers) ──
 * Mirrors `use-summary.ts` so navigation away and back preserves the
 * persisted gap report exactly like the literature review does. */
const gapText = ref<string | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const generatedAt = ref<string | null>(null);

export function useGapAnalysis() {
  async function loadSaved(): Promise<void> {
    try {
      const saved = await tauriCommand<SavedGapAnalysis | null>('get_saved_gap_analysis', {});
      if (saved) {
        gapText.value = saved.gapText;
        generatedAt.value = saved.generatedAt;
      }
    } catch {
      // Silently ignore - saved gap analysis is optional.
    }
  }

  /** Reset all gap-analysis state (called on import or project reset). */
  function clearGapAnalysis(): void {
    gapText.value = null;
    generatedAt.value = null;
    error.value = null;
  }

  async function generate(style: CitationStyle = 'APA'): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const result = await tauriCommand<string>('analyze_research_gaps', {
        citationStyle: style,
      });
      gapText.value = result;
      // The backend saves with a timestamp; reload to get the exact server timestamp.
      await loadSaved();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  /** Format the ISO timestamp using the user's system locale. */
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
    gapText,
    loading,
    error,
    generatedAt,
    loadSaved,
    generate,
    clearGapAnalysis,
    formatGeneratedAt,
  };
}
