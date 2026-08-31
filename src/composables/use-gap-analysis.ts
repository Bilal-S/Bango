import { ref } from 'vue';
import { createSavedReport } from './use-saved-report';

interface SavedGapAnalysis {
  gapText: string;
  citationStyle: string;
  generatedAt: string;
}

/* Premium per-report generation guidance (AI Summary view collapsible card).
 * Mirrors `use-summary.ts`: session-scoped, resets on project import/reset,
 * parsed to a positive integer at generate time (`string | number` because
 * `v-model` on `<input type="number">` auto-casts valid entries to numbers). */
const additionalInstructions = ref('');
const targetWordCount = ref<string | number>('');

/* Module-level singleton state (shared across all callers) via the shared
 * saved-report factory. Mirrors `use-summary.ts` so navigation away and back
 * preserves the persisted gap report exactly like the literature review. */
const report = createSavedReport<SavedGapAnalysis>({
  getCommand: 'get_saved_gap_analysis',
  generateCommand: 'analyze_research_gaps',
  readText: (saved) => saved.gapText,
  onClear: () => {
    additionalInstructions.value = '';
    targetWordCount.value = '';
  },
});

export function useGapAnalysis() {
  return {
    gapText: report.text,
    loading: report.loading,
    error: report.error,
    generatedAt: report.generatedAt,
    additionalInstructions,
    targetWordCount,
    loadSaved: report.loadSaved,
    generate: report.generate,
    clearGapAnalysis: report.clear,
    formatGeneratedAt: report.formatGeneratedAt,
  };
}
