import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { sigmaEvents } from '../helpers/sigma-renderer-stub';
import { nextTick } from 'vue';
import Graph from 'graphology';
import { clusterColor } from '@/types/biblio-network';
import { getTemporalColor } from '@/utils/color';

import CocitationNetworkGraph from '@/components/cocitation-network-graph.vue';

/** Two co-cited papers sharing an edge. */
function makeGraph(): Graph {
  const g = new Graph();
  g.addNode('p1', {
    label: 'Shared Reference A',
    title: 'A seminal paper',
    year: 2015,
    journal: 'Nature',
    citationCount: 15,
    coCitationCount: 2,
    cluster: 0,
  });
  g.addNode('p2', {
    label: 'Shared Reference B',
    title: 'Another seminal paper',
    year: 2021,
    journal: 'Science',
    citationCount: 7,
    coCitationCount: 2,
    cluster: 1,
  });
  g.addEdge('p1', 'p2', { weight: 2 });
  return g;
}

type Props = InstanceType<typeof CocitationNetworkGraph>['$props'];

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    graph: null,
    loading: false,
    isLayouting: false,
    error: null,
    focusedNodeId: null,
    selectedClusters: [],
    colorMode: 'cluster',
    minYear: 2015,
    maxYear: 2021,
    recalculateTrigger: 0,
    ...overrides,
  } as Props;
}

async function mountWithGraph(): Promise<{ wrapper: ReturnType<typeof mount>; graph: Graph }> {
  const wrapper = mount(CocitationNetworkGraph, { props: baseProps() });
  const graph = makeGraph();
  await wrapper.setProps({ graph });
  await vi.waitFor(() => expect(sigmaEvents.has('enterNode')).toBe(true));
  return { wrapper, graph };
}

describe('cocitation-network-graph.vue', () => {
  beforeEach(() => {
    sigmaEvents.clear();
  });

  it('renders_loading_and_error_states', async () => {
    const wrapper = mount(CocitationNetworkGraph, { props: baseProps({ loading: true }) });
    expect(wrapper.text()).toContain('Loading co-citation network');

    await wrapper.setProps({ loading: false, error: 'Threshold recalc failed' });
    expect(wrapper.text()).toContain('Threshold recalc failed');

    await wrapper.find('button').trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);

    await wrapper.setProps({ error: null });
    expect(wrapper.text()).toContain('No co-citation data');
  });

  it('hover_shows_node_tooltip', async () => {
    const { wrapper } = await mountWithGraph();

    const enter = sigmaEvents.get('enterNode') as (p: { node: string }) => void;
    enter({ node: 'p1' });
    await nextTick();

    const text = wrapper.text();
    expect(text).toContain('Shared Reference A');
    expect(text).toContain('A seminal paper');
    expect(text).toContain('Cited by (in-scope):');
    expect(text).toContain('2 articles');
    expect(text).toContain('Total citations:');
    expect(text).toContain('15');
    expect(text).toContain('Year:');
    expect(text).toContain('2015');
    expect(text).toContain('Journal:');
    expect(text).toContain('Nature');
  });

  it('color_mode_switch_updates_node_colors', async () => {
    const { wrapper, graph } = await mountWithGraph();

    expect(graph.getNodeAttribute('p1', 'color')).toBe(clusterColor(0));
    expect(graph.getNodeAttribute('p2', 'color')).toBe(clusterColor(1));

    await wrapper.setProps({ colorMode: 'temporal' });
    await nextTick();

    expect(graph.getNodeAttribute('p1', 'color')).toBe(getTemporalColor(2015, 2015, 2021));
    expect(graph.getNodeAttribute('p2', 'color')).toBe(getTemporalColor(2021, 2015, 2021));
    expect(graph.getNodeAttribute('p1', 'color')).not.toBe(clusterColor(0));
  });
});
