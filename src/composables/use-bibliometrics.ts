import { ref, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface BiblioKpis {
  includedCount: number;
  totalCitations: number;
  uniqueAuthors: number;
  yearFrom: number | null;
  yearTo: number | null;
  avgYear: number | null;
  growthRate: number | null;
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
  avgYear: null,
  growthRate: null,
});
const loading = ref(false);
const refreshing = ref(false);
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

  async function refresh() {
    refreshing.value = true;
    error.value = null;
    try {
      await tauriCommand<NormalizeResult>('biblio_normalize');
      await fetchKpis();
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      refreshing.value = false;
    }
  }

  onMounted(fetchKpis);

  return { kpis, loading, refreshing, error, fetchKpis, refresh };
}
