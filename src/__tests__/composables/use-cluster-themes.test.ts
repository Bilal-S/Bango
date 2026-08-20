import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ref, nextTick } from 'vue';
import { setActivePinia, createPinia } from 'pinia';
import Graph from 'graphology';

vi.mock('@/composables/use-tauri-command', () => ({
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useToast } from '@/composables/use-toast';
import { useClusterThemes } from '@/composables/use-cluster-themes';
import type { ClusterMember } from '@/utils/cluster-members';

const mockedCommand = vi.mocked(tauriCommand);

const MEMBERS: ClusterMember[] = [{ id: 'author-1', label: 'Alice' }];

describe('use-cluster-themes', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    mockedCommand.mockReset();
  });

  it('analyze_invokes_command_and_caches', async () => {
    mockedCommand.mockResolvedValue('# Cluster themes');
    const recalculateTrigger = ref(0);
    const graph = ref<Graph | null>(null);
    const { analyze, entryFor } = useClusterThemes({
      networkType: 'co_authorship',
      recalculateTrigger,
      graph,
    });

    expect(entryFor(5).markdown).toBeNull();
    await analyze(5, MEMBERS);

    expect(mockedCommand).toHaveBeenCalledWith('biblio_analyze_cluster_themes', {
      networkType: 'co_authorship',
      clusterIndex: 5,
      members: MEMBERS,
    });
    expect(entryFor(5).markdown).toBe('# Cluster themes');
    expect(entryFor(6).markdown).toBeNull();
  });

  it('invalidate_watch_uses_array_of_getters', async () => {
    mockedCommand.mockResolvedValue('# Cached');
    const recalculateTrigger = ref(0);
    const graph = ref<Graph | null>(null);
    const { analyze, entryFor } = useClusterThemes({
      networkType: 'co_occurrence',
      recalculateTrigger,
      graph,
    });

    await analyze(1, MEMBERS);
    expect(entryFor(1).markdown).toBe('# Cached');

    // A recalculate bump (onRecalculate / onResetAnalysis / filter re-fetch)
    // must clear the cache through the centralized watch.
    recalculateTrigger.value++;
    await nextTick();
    expect(entryFor(1).markdown).toBeNull();

    // A graph swap (new network fetch => new Louvain indices) clears too.
    await analyze(2, MEMBERS);
    expect(entryFor(2).markdown).toBe('# Cached');
    graph.value = new Graph();
    await nextTick();
    expect(entryFor(2).markdown).toBeNull();
  });

  it('reanalyze_forces_fresh_llm_call', async () => {
    mockedCommand.mockResolvedValue('# First');
    const recalculateTrigger = ref(0);
    const graph = ref<Graph | null>(null);
    const { analyze, reanalyze, entryFor } = useClusterThemes({
      networkType: 'co_authorship',
      recalculateTrigger,
      graph,
    });

    await analyze(3, MEMBERS);
    expect(entryFor(3).markdown).toBe('# First');

    // Re-analyze bypasses the session cache: a fresh LLM call replaces it.
    mockedCommand.mockResolvedValue('# Second');
    await reanalyze(3, MEMBERS);
    expect(mockedCommand).toHaveBeenCalledTimes(2);
    expect(entryFor(3).markdown).toBe('# Second');

    // The refreshed result is now the cached one: no further call.
    await analyze(3, MEMBERS);
    expect(mockedCommand).toHaveBeenCalledTimes(2);
  });

  it('copyMarkdown_reports_outcome_via_toast', async () => {
    const writeText = vi.fn();
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    const { copyMarkdown } = useClusterThemes({
      networkType: 'co_authorship',
      recalculateTrigger: ref(0),
      graph: ref<Graph | null>(null),
    });

    writeText.mockResolvedValue(undefined);
    await copyMarkdown('# Cluster themes');
    expect(writeText).toHaveBeenCalledWith('# Cluster themes');
    const success = useToast().toasts.value;
    expect(success[success.length - 1]?.message).toBe('Thematic analysis copied to clipboard');
    expect(success[success.length - 1]?.type).toBe('success');

    writeText.mockRejectedValue(new Error('clipboard denied'));
    // A rejected clipboard write must resolve (error toast), never reject.
    await expect(copyMarkdown('# Cluster themes')).resolves.toBeUndefined();
    const failed = useToast().toasts.value;
    expect(failed[failed.length - 1]?.message).toBe('Failed to copy to clipboard');
    expect(failed[failed.length - 1]?.type).toBe('error');
  });
});
