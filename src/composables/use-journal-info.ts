import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';
import type { YearCount } from './use-bibliometrics';

/** Full metadata + time-series for one journal_index row. */
export interface JournalInfo {
  id: string;
  journalTitle: string;
  issn: string | null;
  eissn: string | null;
  publisherName: string | null;
  publisherAddress: string | null;
  languages: string | null;
  webOfScienceCategories: string | null;
  /** Number of included articles linked to this journal. */
  articleCount: number;
  firstYear: number | null;
  lastYear: number | null;
  /** This journal's yearly included-article counts (ascending by year). */
  pubsByYear: YearCount[];
  /** SUM(num_cited) across included articles in this journal. */
  citationsTotal: number;
}

/**
 * Lazy loader for a single journal's full metadata + time-series.
 * Mirrors the per-call (non-singleton) pattern of `use-ai-summary.ts`.
 */
export function useJournalInfo() {
  const info = ref<JournalInfo | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function getJournalInfo(journalIndexId: string): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      info.value = await tauriCommand<JournalInfo | null>('biblio_get_journal_info', {
        journalIndexId,
      });
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      info.value = null;
    } finally {
      loading.value = false;
    }
  }

  function clear(): void {
    info.value = null;
    error.value = null;
  }

  return { info, loading, error, getJournalInfo, clear };
}
