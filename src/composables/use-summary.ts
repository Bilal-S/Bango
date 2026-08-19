import { ref } from 'vue';
import { createSavedReport } from './use-saved-report';

export type CitationStyle = 'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA';

interface SavedSummary {
  summaryText: string;
  citationStyle: string;
  generatedAt: string;
}

/* Module-level singleton citation style (persisted alongside the summary). */
const citationStyle = ref<CitationStyle>('APA');

/* Module-level singleton state (shared across all callers) via the shared
 * saved-report factory; the citation style rides along via the hooks. */
const report = createSavedReport<SavedSummary>({
  getCommand: 'get_saved_summary',
  generateCommand: 'generate_summary',
  readText: (saved) => saved.summaryText,
  onLoaded: (saved) => {
    if (saved.citationStyle) {
      citationStyle.value = saved.citationStyle as CitationStyle;
    }
  },
  onClear: () => {
    citationStyle.value = 'APA';
  },
});

export function useSummary() {
  return {
    summaryText: report.text,
    loading: report.loading,
    error: report.error,
    generatedAt: report.generatedAt,
    citationStyle,
    loadSaved: report.loadSaved,
    generate: (style: CitationStyle = 'APA') => report.generate(style),
    clearSummary: report.clear,
    formatGeneratedAt: report.formatGeneratedAt,
  };
}
