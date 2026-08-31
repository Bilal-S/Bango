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

/* Premium per-report generation guidance (AI Summary view collapsible card).
 * Session-scoped: survives navigation, resets on project import/reset. The
 * word count is held as `string | number` because `v-model` on
 * `<input type="number">` auto-casts valid entries to numbers; it is parsed to
 * a positive integer at generate time. */
const additionalInstructions = ref('');
const targetWordCount = ref<string | number>('');

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
    additionalInstructions.value = '';
    targetWordCount.value = '';
  },
});

export function useSummary() {
  return {
    summaryText: report.text,
    loading: report.loading,
    error: report.error,
    generatedAt: report.generatedAt,
    citationStyle,
    additionalInstructions,
    targetWordCount,
    loadSaved: report.loadSaved,
    generate: report.generate,
    clearSummary: report.clear,
    formatGeneratedAt: report.formatGeneratedAt,
  };
}
