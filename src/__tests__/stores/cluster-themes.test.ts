import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { clusterThemesKey, useClusterThemesStore } from '@/stores/cluster-themes';

const mockedCommand = vi.mocked(tauriCommand);

describe('cluster-themes store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    mockedCommand.mockReset();
  });

  it('cache_keyed_by_network_and_cluster', async () => {
    mockedCommand.mockResolvedValue('# Themes');
    const store = useClusterThemesStore();

    await store.analyze('co_authorship', 0, []);
    await store.analyze('co_occurrence', 1, []);

    expect(mockedCommand).toHaveBeenCalledWith('biblio_analyze_cluster_themes', {
      networkType: 'co_authorship',
      clusterIndex: 0,
      members: [],
    });
    expect(mockedCommand).toHaveBeenCalledWith('biblio_analyze_cluster_themes', {
      networkType: 'co_occurrence',
      clusterIndex: 1,
      members: [],
    });
    expect(Object.keys(store.entries)).toEqual([
      clusterThemesKey('co_authorship', 0),
      clusterThemesKey('co_occurrence', 1),
    ]);
    expect(store.entry(clusterThemesKey('co_authorship', 0)).markdown).toBe('# Themes');
    expect(store.entry(clusterThemesKey('co_occurrence', 1)).markdown).toBe('# Themes');
  });

  it('invalidate_clears_cache', async () => {
    mockedCommand.mockResolvedValue('# Themes');
    const store = useClusterThemesStore();
    await store.analyze('co_authorship', 2, []);
    expect(store.entry(clusterThemesKey('co_authorship', 2)).markdown).toBe('# Themes');

    store.invalidate();
    expect(store.entries).toEqual({});
    expect(store.entry(clusterThemesKey('co_authorship', 2)).markdown).toBeNull();

    // Single-key invalidate only clears that key.
    await store.analyze('co_authorship', 2, []);
    await store.analyze('co_occurrence', 3, []);
    store.invalidate(clusterThemesKey('co_authorship', 2));
    expect(store.entry(clusterThemesKey('co_authorship', 2)).markdown).toBeNull();
    expect(store.entry(clusterThemesKey('co_occurrence', 3)).markdown).toBe('# Themes');
  });

  it('drops_stale_result_after_invalidate', async () => {
    let resolveFn!: (value: string) => void;
    mockedCommand.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveFn = resolve;
      })
    );
    const store = useClusterThemesStore();

    const pending = store.analyze('co_authorship', 0, []);
    const inflight = store.entry(clusterThemesKey('co_authorship', 0));
    expect(inflight.loading).toBe(true);

    // Recalculate fires while the LLM call is in flight.
    store.invalidate();

    resolveFn('# Late response');
    await pending;

    const entry = store.entry(clusterThemesKey('co_authorship', 0));
    expect(entry.markdown).toBeNull();
    expect(entry.loading).toBe(false);
    expect(entry.error).toBeNull();
  });

  it('analyze_reuses_cached_result_without_recalling', async () => {
    mockedCommand.mockResolvedValue('# Themes');
    const store = useClusterThemesStore();

    const first = await store.analyze('co_authorship', 0, []);
    expect(mockedCommand).toHaveBeenCalledTimes(1);

    // Clicking Analyze on an already-analyzed cluster redisplays the cached
    // markdown - no second LLM call.
    const second = await store.analyze('co_authorship', 0, []);
    expect(mockedCommand).toHaveBeenCalledTimes(1);
    expect(second).toBe(first);
    expect(second).toBe('# Themes');
    const entry = store.entry(clusterThemesKey('co_authorship', 0));
    expect(entry.loading).toBe(false);
    expect(entry.markdown).toBe('# Themes');
  });

  it('analyze_skips_duplicate_inflight_call', async () => {
    let resolveFn!: (value: string) => void;
    mockedCommand.mockReturnValue(
      new Promise<string>((resolve) => {
        resolveFn = resolve;
      })
    );
    const store = useClusterThemesStore();

    // Two clicks while the same cluster's analysis is in flight: one call only.
    const first = store.analyze('co_authorship', 5, []);
    const second = store.analyze('co_authorship', 5, []);
    resolveFn('# Only call');
    const [a, b] = await Promise.all([first, second]);

    expect(mockedCommand).toHaveBeenCalledTimes(1);
    expect(a).toBe('# Only call');
    expect(b).toBeNull(); // duplicate skipped; the entry resolves via the first
    const entry = store.entry(clusterThemesKey('co_authorship', 5));
    expect(entry.markdown).toBe('# Only call');
    expect(entry.loading).toBe(false);
  });

  it('analyze_drops_stale_response_when_key_reanalyzed', async () => {
    // Two deferred responses: the first (stale) request resolves while the
    // replacement for the same key is still in flight.
    const resolvers: Array<(value: string) => void> = [];
    mockedCommand.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolvers.push(resolve);
        })
    );
    const store = useClusterThemesStore();
    const key = clusterThemesKey('co_authorship', 0);

    const first = store.analyze('co_authorship', 0, []); // generation 1
    // The re-analyze path: invalidate the key, then start a replacement.
    store.invalidate(key);
    const second = store.analyze('co_authorship', 0, []); // generation 2
    expect(store.entry(key).loading).toBe(true);

    // The STALE response arrives while the fresh request is still running:
    // it must be discarded (old guard let it clobber the loading entry).
    resolvers[0]!('# Stale');
    await first;
    expect(store.entry(key).markdown).toBeNull();
    expect(store.entry(key).loading).toBe(true);

    // The fresh response lands.
    resolvers[1]!('# Fresh');
    await second;
    expect(store.entry(key).markdown).toBe('# Fresh');
    expect(store.entry(key).loading).toBe(false);
  });

  it('analyze_retries_after_error', async () => {
    mockedCommand.mockRejectedValueOnce(new Error('LLM down'));
    const store = useClusterThemesStore();

    await store.analyze('co_authorship', 7, []);
    const errored = store.entry(clusterThemesKey('co_authorship', 7));
    expect(errored.error).toBe('LLM down');

    // An errored entry is NOT served from cache: the next click retries.
    mockedCommand.mockResolvedValue('# Recovered');
    const retried = await store.analyze('co_authorship', 7, []);
    expect(mockedCommand).toHaveBeenCalledTimes(2);
    expect(retried).toBe('# Recovered');
    expect(store.entry(clusterThemesKey('co_authorship', 7)).error).toBeNull();
  });
});
