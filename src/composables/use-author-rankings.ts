import { ref, onMounted } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface AuthorRank {
  id: string;
  displayName: string;
  normalizedName: string;
  articleCount: number;
  firstAuthorCount: number;
  lastAuthorCount: number;
  soloPaperCount: number;
  totalCitations: number;
  estimatedHIndex: number;
  i10Index: number;
  gIndex: number;
  avgCitationsPerPaper: number | null;
  avgYear: number | null;
  yearsActive: number | null;
  productivityRate: number | null;
  recentPaperCount: number;
  primaryInstitution: string | null;
}

export interface AuthorProductivityKpis {
  totalAuthors: number;
  totalPapers: number;
  avgHIndex: number | null;
  maxHIndex: number;
  avgCitations: number | null;
  totalCollaborations: number;
  yearFrom: number | null;
  yearTo: number | null;
}

/**
 * Singleton composable for the Author Productivity view.
 * Loads all author rankings + aggregate KPIs once on mount.
 * Mirrors the `use-bibliometrics.ts` singleton pattern.
 */
export function useAuthorRankings() {
  const rankings = ref<AuthorRank[]>([]);
  const kpis = ref<AuthorProductivityKpis | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function fetchRankings(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const [rankingsData, kpisData] = await Promise.all([
        tauriCommand<AuthorRank[]>('biblio_get_author_rankings'),
        tauriCommand<AuthorProductivityKpis>('biblio_get_author_productivity_kpis'),
      ]);
      rankings.value = rankingsData;
      kpis.value = kpisData;
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  onMounted(() => {
    loading.value = true;
    setTimeout(() => {
      void fetchRankings();
    }, 0);
  });

  return { rankings, kpis, loading, error, fetchRankings };
}
