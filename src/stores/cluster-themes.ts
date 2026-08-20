import { defineStore } from 'pinia';
import { tauriCommand } from '@/composables/use-tauri-command';
import type { ClusterMember } from '@/utils/cluster-members';

/** Networks supported by cluster thematic analysis. */
export type ClusterThemesNetworkType = 'co_authorship' | 'co_occurrence';

/** Per-key cache entry: exactly one in-flight/resolved analysis per cluster. */
export interface ClusterThemesEntry {
  markdown: string | null;
  loading: boolean;
  error: string | null;
}

const EMPTY_ENTRY: ClusterThemesEntry = { markdown: null, loading: false, error: null };

/** Cache key: `networkType:clusterIndex` (Louvain indices are per network). */
export function clusterThemesKey(
  networkType: ClusterThemesNetworkType,
  clusterIndex: number
): string {
  return `${networkType}:${clusterIndex}`;
}

/**
 * Session-only cache of cluster thematic analyses (no persistence; the plan
 * keys by `networkType:clusterIndex` and invalidates on every recalculate
 * because Louvain indices are not stable across runs).
 */
export const useClusterThemesStore = defineStore('clusterThemes', {
  state: () => ({
    entries: {} as Record<string, ClusterThemesEntry>,
    /**
     * Per-key request generation. A response writes back only when the key's
     * generation still matches the one captured when its request started, so
     * a stale response can never overwrite an entry that was invalidated AND
     * replaced meanwhile (e.g. re-analyze while an older call is in flight).
     */
    generations: {} as Record<string, number>,
  }),
  getters: {
    /** Entry for a key; a stable empty object for unknown keys. */
    entry(state) {
      return (key: string): ClusterThemesEntry => state.entries[key] ?? { ...EMPTY_ENTRY };
    },
  },
  actions: {
    /**
     * Invoke the backend command and cache the result under `key`.
     * Session-cache short-circuit: a resolved entry is redisplayed without a
     * new LLM call, and a second click while a call is in flight is a no-op
     * instead of a duplicate request. The panel's re-analyze (the composable's
     * `reanalyze`) deletes the entry first, which is the explicit refresh
     * path. An errored entry (markdown null) falls through to a retry.
     *
     * Stale-result dropping (generation token): a write-back requires BOTH
     * the entry to still be loading AND the key's generation to match the
     * one captured at call time. Invalidation alone deletes the entry (first
     * check fails); invalidation followed by a replacement request bumps the
     * generation (second check fails). Either way the late response is
     * discarded instead of clobbering the fresh entry.
     */
    async analyze(
      networkType: ClusterThemesNetworkType,
      clusterIndex: number,
      members: ClusterMember[]
    ): Promise<string | null> {
      const key = clusterThemesKey(networkType, clusterIndex);
      const existing = this.entries[key];
      if (existing?.markdown) return existing.markdown;
      if (existing?.loading) return null;
      const generation = (this.generations[key] ?? 0) + 1;
      this.generations[key] = generation;
      this.entries[key] = { markdown: null, loading: true, error: null };
      try {
        const markdown = await tauriCommand<string>('biblio_analyze_cluster_themes', {
          networkType,
          clusterIndex,
          members,
        });
        if (this.entries[key]?.loading && this.generations[key] === generation) {
          this.entries[key] = { markdown, loading: false, error: null };
        }
        return markdown;
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        if (this.entries[key]?.loading && this.generations[key] === generation) {
          this.entries[key] = { markdown: null, loading: false, error: message };
        }
        return null;
      }
    },
    /** Clear one entry, or the whole map when no key is given. */
    invalidate(key?: string): void {
      if (key === undefined) {
        this.entries = {};
      } else {
        delete this.entries[key];
      }
    },
  },
});
