import { watch, type Ref } from 'vue';
import type Graph from 'graphology';
import {
  clusterThemesKey,
  useClusterThemesStore,
  type ClusterThemesEntry,
  type ClusterThemesNetworkType,
} from '@/stores/cluster-themes';
import { useToast } from '@/composables/use-toast';
import type { ClusterMember } from '@/utils/cluster-members';

export interface UseClusterThemesOptions {
  /** The hosting view's network type; part of the cache key. */
  networkType: ClusterThemesNetworkType;
  /** Bumped by `onRecalculate`, `onResetAnalysis`, and filter re-fetches. */
  recalculateTrigger: Ref<number>;
  /** The network graph; a new graph instance means new cluster indices. */
  graph: Ref<Graph | null>;
}

/**
 * View-facing composable over the `cluster-themes` Pinia store.
 *
 * Centralized invalidation: installs ONE multi-source watch in the
 * array-of-getters form (`src/AGENTS.md` Local Contracts - a getter returning
 * a fresh array is a known infinite-loop regression). Any `recalculateTrigger`
 * bump or graph swap clears the cache; no per-view `invalidate()` call sites.
 */
export function useClusterThemes(options: UseClusterThemesOptions) {
  const store = useClusterThemesStore();
  const toast = useToast();

  watch([() => options.recalculateTrigger.value, () => options.graph.value], () =>
    store.invalidate()
  );

  /** Reactive entry for one cluster index of this view's network. */
  function entryFor(clusterIndex: number): ClusterThemesEntry {
    return store.entry(clusterThemesKey(options.networkType, clusterIndex));
  }

  /** Run (or reuse) the analysis for one cluster. */
  async function analyze(clusterIndex: number, members: ClusterMember[]): Promise<string | null> {
    return store.analyze(options.networkType, clusterIndex, members);
  }

  /** Force a fresh analysis for one cluster (clears its cache entry first). */
  async function reanalyze(clusterIndex: number, members: ClusterMember[]): Promise<string | null> {
    store.invalidate(clusterThemesKey(options.networkType, clusterIndex));
    return analyze(clusterIndex, members);
  }

  /** Copy-only export (decision D3): markdown to the clipboard. A rejected
   * write surfaces as an error toast, never an unhandled rejection
   * (search-strategy-card precedent). */
  async function copyMarkdown(markdown: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(markdown);
      toast.show('Thematic analysis copied to clipboard', 'success');
    } catch {
      toast.show('Failed to copy to clipboard', 'error');
    }
  }

  return { analyze, reanalyze, entryFor, copyMarkdown };
}
