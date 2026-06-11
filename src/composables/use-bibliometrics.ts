import { ref, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface YearCount {
  year: number;
  count: number;
}

export interface BiblioKpis {
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
});
const loading = ref(false);
const normalizing = ref(false);
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
    // Use setTimeout(0) — a macro-task that yields to the browser's render
    // pipeline — so the progress bar actually paints before the IPC call starts.
    await new Promise<void>((r) => setTimeout(r, 0));
    try {
      await tauriCommand<NormalizeResult>('biblio_normalize');
      await fetchKpis();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
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
      // Auto-normalize only when we have included articles AND no normalized data yet.
      // The user can trigger re-normalization manually via the Refresh button.
      if (kpis.value.includedCount > 0 && kpis.value.uniqueAuthors === 0) {
        runNormalization(); // not awaited — UI stays responsive
      }
    }, 0);
  });

  return { kpis, loading, normalizing, error, fetchKpis, runNormalization };
}
