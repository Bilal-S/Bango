import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import Graph from 'graphology';
import { clusterColor } from '@/types/biblio-network';

/* Sigma needs WebGL, which happy-dom cannot provide. Stub the renderer
 * composable (per docs/CLAUDE.md component-test rule) and expose the event
 * handler registry so tests can drive hover events like the real renderer. */
const sigmaEvents = vi.hoisted(() => new Map<string, (payload: unknown) => void>());

vi.mock('@/composables/use-sigma-renderer', () => {
  interface FakeRenderer {
    on: (type: string, cb: (payload: unknown) => void) => void;
    refresh: () => void;
    kill: () => void;
  }
  const rendererRef: { value: FakeRenderer | null } = { value: null };
  return {
    useSigmaRenderer: () => ({
      renderer: rendererRef,
      initRenderer: () => {
        rendererRef.value = {
          on: (type, cb) => sigmaEvents.set(type, cb),
          refresh: () => {},
          kill: () => {},
        };
        return rendererRef.value;
      },
      destroyRenderer: () => {
        rendererRef.value = null;
      },
      locateNode: () => {},
      resetZoom: () => {},
      refresh: () => {},
    }),
  };
});

import KeywordNetworkGraph from '@/components/keyword-network-graph.vue';

function makeGraph(): Graph {
  const g = new Graph();
  g.addNode('k1', {
    label: 'machine learning',
    weight: 5,
    source: 'metadata',
    avgYear: 2020,
    yearCounts: [],
    rawTerms: ['machine learning', 'ml'],
    cluster: 0,
  });
  g.addNode('k2', {
    label: 'systematic review',
    weight: 3,
    source: 'tags',
    avgYear: 2021,
    yearCounts: [],
    rawTerms: ['systematic review'],
    cluster: 1,
  });
  g.addEdge('k1', 'k2', { weight: 2 });
  return g;
}

type Props = InstanceType<typeof KeywordNetworkGraph>['$props'];

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    graph: null,
    loading: false,
    isLayouting: false,
    error: null,
    focusedNodeId: null,
    selectedClusters: [],
    colorMode: 'cluster',
    minYear: 2019,
    maxYear: 2022,
    recalculateTrigger: 0,
    ...overrides,
  } as Props;
}

describe('keyword-network-graph.vue', () => {
  beforeEach(() => {
    sigmaEvents.clear();
  });

  it('renders_loading_and_error_states', async () => {
    const wrapper = mount(KeywordNetworkGraph, { props: baseProps({ loading: true }) });
    expect(wrapper.text()).toContain('Loading keyword network');

    await wrapper.setProps({ loading: false, error: 'Normalization failed' });
    expect(wrapper.text()).toContain('Normalization failed');

    await wrapper.find('button').trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);

    await wrapper.setProps({ error: null });
    expect(wrapper.text()).toContain('No keyword data matched');
  });

  it('hover_shows_node_tooltip', async () => {
    const wrapper = mount(KeywordNetworkGraph, { props: baseProps() });
    const graph = makeGraph();
    await wrapper.setProps({ graph });
    await vi.waitFor(() => expect(sigmaEvents.has('enterNode')).toBe(true));

    const enter = sigmaEvents.get('enterNode') as (p: { node: string }) => void;
    enter({ node: 'k1' });
    await nextTick();

    const text = wrapper.text();
    expect(text).toContain('machine learning');
    expect(text).toContain('Occurrences:');
    expect(text).toContain('5 docs');
    expect(text).toContain('Source:');
    /* The `capitalize` modifier is CSS-only (text-transform); textContent keeps
     * the raw source value. */
    expect(text).toContain('metadata');
    expect(text).toContain('Raw Terms:');
    expect(text).toContain('machine learning, ml');

    /* Cluster colors are applied to the graph on init. */
    expect(graph.getNodeAttribute('k1', 'color')).toBe(clusterColor(0));
  });
});
