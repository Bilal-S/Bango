import { createSavedReport } from './use-saved-report';
import type { CitationStyle } from './use-summary';

interface SavedGapAnalysis {
  gapText: string;
  citationStyle: string;
  generatedAt: string;
}

/* Module-level singleton state (shared across all callers) via the shared
 * saved-report factory. Mirrors `use-summary.ts` so navigation away and back
 * preserves the persisted gap report exactly like the literature review. */
const report = createSavedReport<SavedGapAnalysis>({
  getCommand: 'get_saved_gap_analysis',
  generateCommand: 'analyze_research_gaps',
  readText: (saved) => saved.gapText,
});

export function useGapAnalysis() {
  return {
    gapText: report.text,
    loading: report.loading,
    error: report.error,
    generatedAt: report.generatedAt,
    loadSaved: report.loadSaved,
    generate: (style: CitationStyle = 'APA') => report.generate(style),
    clearGapAnalysis: report.clear,
    formatGeneratedAt: report.formatGeneratedAt,
  };
}
