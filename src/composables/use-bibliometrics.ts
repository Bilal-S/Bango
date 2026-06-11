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
    try {
      await tauriCommand<NormalizeResult>('biblio_normalize');
      await fetchKpis();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      normalizing.value = false;
    }
  }

  onMounted(async () => {
    // First fetch KPIs to check if we have included articles
    await fetchKpis();
    // Auto-normalize when we have included articles
    if (kpis.value.includedCount > 0) {
      await runNormalization();
    }
  });

  return { kpis, loading, normalizing, error, fetchKpis, runNormalization };
}
