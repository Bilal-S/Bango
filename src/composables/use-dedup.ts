import { ref } from 'vue';
import { tauriCommand } from './use-tauri-command';

export interface DuplicatePair {
  articleAId: string;
  articleBId: string;
  articleATitle: string;
  articleBTitle: string;
  articleAAuthors: string[];
  articleBAuthors: string[];
  articleAYear: number | null;
  articleBYear: number | null;
  articleASource?: string;
  articleBSource?: string;
  similarity: number;
  matchType: 'exactDuplicate' | 'fuzzyMatch';
  strategy: string;
}

export interface DedupResult {
  exactDuplicates: DuplicatePair[];
  fuzzyMatches: DuplicatePair[];
  autoMergedCount: number;
  needsReviewCount: number;
}

export type DedupResolution = 'keepA' | 'keepB' | 'keepBoth';

export function useDedup() {
  const result = ref<DedupResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const resolvedCount = ref(0);
  const mergedCount = ref(0);

  /** Detection only — does NOT modify the database. */
  async function checkDuplicates(): Promise<DedupResult | null> {
    loading.value = true;
    error.value = null;
    try {
      result.value = await tauriCommand<DedupResult>('check_duplicates');
      return result.value;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      return null;
    } finally {
      loading.value = false;
    }
  }

  /** User-triggered: merge all high-confidence exact duplicates. */
  async function mergeAllExact(): Promise<number> {
    if (!result.value?.exactDuplicates.length) return 0;

    loading.value = true;
    error.value = null;
    try {
      const count = await tauriCommand<number>('merge_exact_duplicates', {
        request: { pairs: result.value.exactDuplicates },
      });
      mergedCount.value += count;

      // Re-check from backend to get authoritative state after merge
      await checkDuplicates();

      return count;
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
      return 0;
    } finally {
      loading.value = false;
    }
  }

  async function resolveFuzzy(pair: DuplicatePair, resolution: DedupResolution): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      await tauriCommand('resolve_fuzzy_match', {
        request: {
          pairIndex: 0,
          resolution,
          articleAId: pair.articleAId,
          articleBId: pair.articleBId,
        },
      });
      resolvedCount.value++;

      // Remove resolved pair from fuzzy matches
      if (result.value) {
        result.value.fuzzyMatches = result.value.fuzzyMatches.filter(
          (p) => !(p.articleAId === pair.articleAId && p.articleBId === pair.articleBId)
        );
        result.value.needsReviewCount = result.value.fuzzyMatches.length;
      }
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      loading.value = false;
    }
  }

  return {
    result,
    loading,
    error,
    resolvedCount,
    mergedCount,
    checkDuplicates,
    mergeAllExact,
    resolveFuzzy,
  };
}
