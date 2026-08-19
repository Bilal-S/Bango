import { describe, it, expect } from 'vitest';
import Graph from 'graphology';
import {
  createBiblioNetworkState,
  networkErrorMessage,
  runNetworkFetch,
  scaleToRange,
} from '@/composables/use-biblio-network-fetch';

describe('use-biblio-network-fetch', () => {
  it('create_state_initializes_empty_refs_and_zero_counts', () => {
    const state = createBiblioNetworkState();
    expect(state.graph.value).toBeNull();
    expect(state.loading.value).toBe(false);
    expect(state.error.value).toBeNull();
    expect(state.nodeCount.value).toBe(0);
    expect(state.edgeCount.value).toBe(0);
  });

  it('create_state_counts_track_graph_order_and_size', () => {
    const state = createBiblioNetworkState();
    const g = new Graph();
    g.addNode('a');
    g.addNode('b');
    g.addEdge('a', 'b');
    state.graph.value = g;
    expect(state.nodeCount.value).toBe(2);
    expect(state.edgeCount.value).toBe(1);
  });

  it('run_fetch_stores_built_graph_and_resets_loading', async () => {
    const state = createBiblioNetworkState();
    const built = new Graph();
    built.addNode('n1', { label: 'node one' });

    await runNetworkFetch(state, async () => built);

    /* ref() deeply wraps the Graph in a reactive proxy (historical behavior),
       so assert structurally rather than by identity. */
    expect(state.graph.value?.order).toBe(1);
    expect(state.graph.value?.getNodeAttribute('n1', 'label')).toBe('node one');
    expect(state.loading.value).toBe(false);
    expect(state.error.value).toBeNull();
  });

  it('run_fetch_empty_result_stores_null_graph', async () => {
    const state = createBiblioNetworkState();

    await runNetworkFetch(state, async () => null);

    expect(state.graph.value).toBeNull();
    expect(state.loading.value).toBe(false);
    expect(state.error.value).toBeNull();
  });

  it('run_fetch_error_captures_message_clears_graph_and_runs_on_catch', async () => {
    const state = createBiblioNetworkState();
    const built = new Graph();
    built.addNode('n1');
    state.graph.value = built;
    let caught = false;

    await runNetworkFetch(
      state,
      async () => {
        throw new Error('backend exploded');
      },
      () => {
        caught = true;
      }
    );

    expect(state.error.value).toBe('backend exploded');
    expect(state.graph.value).toBeNull();
    expect(state.loading.value).toBe(false);
    expect(caught).toBe(true);
  });

  it('network_error_message_normalizes_error_and_primitive', () => {
    expect(networkErrorMessage(new Error('boom'))).toBe('boom');
    expect(networkErrorMessage('plain string')).toBe('plain string');
    expect(networkErrorMessage(42)).toBe('42');
  });

  it('scale_to_range_maps_linearly', () => {
    expect(scaleToRange(5, 0, 10, 0, 100)).toBe(50);
    expect(scaleToRange(0, 0, 10, 2, 6)).toBe(2);
    expect(scaleToRange(10, 0, 10, 2, 6)).toBe(6);
  });

  it('scale_to_range_returns_midpoint_for_degenerate_range', () => {
    expect(scaleToRange(7, 4, 4, 1, 9)).toBe(5);
  });
});
