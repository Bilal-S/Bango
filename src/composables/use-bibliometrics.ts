import { ref, onMounted } from 'vue';
import { isTauri, tauriCommand } from './use-tauri-command';

export interface YearCount {
  year: number;
  count: number;
}

interface JournalYearData {
  /** Canonical `journal_title` when `journalIndexId` is set; else normalized raw title. */
  journal: string;
  year: number;
  count: number;
  /** `null` → raw fallback (not matched to journal_index). */
  journalIndexId: string | null;
}

interface BiblioKpis {
  includedCount: number;
  totalCitations: number;
  uniqueAuthors: number;
  yearFrom: number | null;
  yearTo: number | null;
  pubsPerYear: number | null;
  pubsByYear: YearCount[];
  avgGrowthRate: number | null;
  refsByYear: YearCount[];
  citationsByYear: YearCount[];
  journalDistribution: JournalYearData[];
}

interface NormalizeResult {
  authors: number;
  terms: number;
  status: {
    authorCount: number;
    institutionCount: number;
    termCount: number;
    articleAuthorLinks: number;
    articleTermLinks: number;
    networkCount: number;
  };
}

const kpis = ref<BiblioKpis>({
  includedCount: 0,
  totalCitations: 0,
  uniqueAuthors: 0,
  yearFrom: null,
  yearTo: null,
  pubsPerYear: null,
  pubsByYear: [],
  avgGrowthRate: null,
  refsByYear: [],
  citationsByYear: [],
  journalDistribution: [],
});
const loading = ref(false);
const normalizing = ref(false);
const progress = ref(0);
const error = ref<string | null>(null);

export function useBibliometrics() {
  async function fetchKpis() {
    loading.value = true;
    error.value = null;
    try {
      kpis.value = await tauriCommand<BiblioKpis>('biblio_get_kpis');
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  /** Whether the backend reports that bibliometric data is stale. */
  async function fetchNeedsRefresh(): Promise<boolean> {
    try {
      return await tauriCommand<boolean>('biblio_get_needs_refresh');
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      return false;
    }
  }

  /**
   * Unified normalization flow:
   * 1. Show progress overlay (normalizing = true)
   * 2. Run biblio_normalize (generic, extensible)
   * 3. Fetch fresh KPIs
   * 4. Hide overlay
   */
  async function runNormalization() {
    normalizing.value = true;
    error.value = null;
    progress.value = 0;

    let unlisten: (() => void) | null = null;
    if (isTauri()) {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<{ step: number; totalSteps: number; message: string }>(
          'biblio:progress',
          (event) => {
            const stepDelta = 100 / event.payload.totalSteps;
            progress.value = event.payload.step * stepDelta;
          }
        );
      } catch {
        // Fallback if event listening fails
      }
    }

    // Use setTimeout(0) - a macro-task that yields to the browser's render
    // pipeline - so the progress bar actually paints before the IPC call starts.
    await new Promise<void>((r) => setTimeout(r, 0));
    try {
      await tauriCommand<NormalizeResult>('biblio_normalize');
      progress.value = 100;
      await fetchKpis();
      // Keep progress bar at 100% for 500ms before returning to Refresh button
      await new Promise<void>((r) => setTimeout(r, 500));
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      if (unlisten) {
        unlisten();
      }
      normalizing.value = false;
    }
  }

  onMounted(() => {
    // Set loading synchronously so spinners render on the very first paint.
    loading.value = true;

    // setTimeout(0) is a macro-task that yields to the browser's paint cycle.
    // This ensures the page renders with spinners BEFORE any IPC calls execute.
    setTimeout(async () => {
      await fetchKpis();
      // Start the normalization/refresh cycle when the persisted stale flag is on.
      // Mutations that affect bibliometrics (imports, references/citations, tag
      // and label edits, status changes, AI screening) set this flag on the
      // backend; biblio_normalize clears it once the transaction commits.
      const needsRefresh = await fetchNeedsRefresh();
      if (kpis.value.includedCount > 0 && needsRefresh) {
        runNormalization(); // not awaited - UI stays responsive
      }
    }, 0);
  });

  return {
    kpis,
    loading,
    normalizing,
    progress,
    error,
    fetchKpis,
    fetchNeedsRefresh,
    runNormalization,
  };
}
