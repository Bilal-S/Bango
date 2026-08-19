import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import Graph from 'graphology';
import { clusterColor } from '@/types/biblio-network';
import { getTemporalColor } from '@/utils/color';

/* Sigma needs WebGL, which happy-dom cannot provide. Stub the renderer
 * composable (per docs/CLAUDE.md component-test rule) and expose the event
 * handler registry so tests can drive events like the real renderer. */
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

import NetworkGraph from '@/components/network-graph.vue';

/** Two co-authors that share a paper. */
function makeGraph(): Graph {
  const g = new Graph();
  g.addNode('a1', {
    label: 'Smith, J.',
    weight: 4,
    totalCitations: 120,
    avgYear: 2019,
    cluster: 0,
  });
  g.addNode('a2', {
    label: 'Doe, A.',
    weight: 2,
    totalCitations: 40,
    avgYear: 2021,
    cluster: 1,
  });
  g.addEdge('a1', 'a2', { weight: 2 });
  return g;
}

type Props = InstanceType<typeof NetworkGraph>['$props'];

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
    maxYear: 2021,
    recalculateTrigger: 0,
    ...overrides,
  } as Props;
}

describe('network-graph.vue (co-author)', () => {
  beforeEach(() => {
    sigmaEvents.clear();
  });

  it('renders_loading_and_error_states', async () => {
    const wrapper = mount(NetworkGraph, { props: baseProps({ loading: true }) });
    expect(wrapper.text()).toContain('Loading network');

    await wrapper.setProps({ loading: false, error: 'Co-author graph failed' });
    expect(wrapper.text()).toContain('Co-author graph failed');

    await wrapper.find('button').trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);

    await wrapper.setProps({ error: null });
    expect(wrapper.text()).toContain('No network data');
  });

  it('color_mode_switch_updates_node_colors', async () => {
    const wrapper = mount(NetworkGraph, { props: baseProps() });
    const graph = makeGraph();
    await wrapper.setProps({ graph });
    await vi.waitFor(() => expect(sigmaEvents.has('enterNode')).toBe(true));

    expect(graph.getNodeAttribute('a1', 'color')).toBe(clusterColor(0));
    expect(graph.getNodeAttribute('a2', 'color')).toBe(clusterColor(1));

    await wrapper.setProps({ colorMode: 'temporal' });
    await nextTick();

    /* Temporal mode interpolates by each author's avgYear. */
    expect(graph.getNodeAttribute('a1', 'color')).toBe(getTemporalColor(2019, 2019, 2021));
    expect(graph.getNodeAttribute('a2', 'color')).toBe(getTemporalColor(2021, 2019, 2021));
    expect(graph.getNodeAttribute('a1', 'color')).not.toBe(clusterColor(0));
  });
});
