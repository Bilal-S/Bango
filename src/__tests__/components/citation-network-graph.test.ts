import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import Graph from 'graphology';
import { citationClusterColor } from '@/types/biblio-citation';
import { getTemporalColor } from '@/utils/color';

/* Sigma needs WebGL, which happy-dom cannot provide. Stub the renderer
 * composable and expose the event handler registry so tests can drive hover
 * events like the real renderer (docs/CLAUDE.md component-test rule). */
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

import CitationNetworkGraph from '@/components/citation-network-graph.vue';

/** Minimal directed citation chain: n1 cites n2, n2 cites n3 (source cites target). */
function makeGraph(): Graph {
  const g = new Graph({ type: 'directed' });
  g.addNode('n1', {
    label: 'Alpha Paper',
    title: 'Foundations of the field',
    numCited: 5,
    numReferences: 3,
    year: 2018,
    cluster: 0,
  });
  g.addNode('n2', {
    label: 'Beta Paper',
    title: 'Modern methods',
    numCited: 9,
    numReferences: 2,
    year: 2020,
    cluster: 1,
  });
  g.addNode('n3', {
    label: 'Gamma Paper',
    title: 'Recent frontiers',
    numCited: 1,
    numReferences: 1,
    year: 2022,
    cluster: 2,
  });
  g.addEdge('n1', 'n2');
  g.addEdge('n2', 'n3');
  return g;
}

type Props = InstanceType<typeof CitationNetworkGraph>['$props'];

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    graph: null,
    loading: false,
    isLayouting: false,
    error: null,
    focusedNodeId: null,
    selectedClusters: [],
    colorMode: 'cluster',
    minYear: 2018,
    maxYear: 2022,
    recalculateTrigger: 0,
    isolationMode: null,
    mainPathNodes: new Set<string>(),
    mainPathEdges: new Set<string>(),
    showMainPath: false,
    ...overrides,
  } as Props;
}

/** Mount first (graph=null), then feed the graph so the watcher fires, like the
 * real parent that passes null while loading. */
async function mountWithGraph(): Promise<{ wrapper: ReturnType<typeof mount>; graph: Graph }> {
  const wrapper = mount(CitationNetworkGraph, { props: baseProps() });
  const graph = makeGraph();
  await wrapper.setProps({ graph });
  await vi.waitFor(() => expect(sigmaEvents.has('enterNode')).toBe(true));
  return { wrapper, graph };
}

describe('citation-network-graph.vue', () => {
  beforeEach(() => {
    sigmaEvents.clear();
  });

  it('renders_loading_and_error_states', async () => {
    const wrapper = mount(CitationNetworkGraph, { props: baseProps({ loading: true }) });
    expect(wrapper.text()).toContain('Loading citation network');

    await wrapper.setProps({ loading: false, isLayouting: true });
    expect(wrapper.text()).toContain('Computing layout');

    await wrapper.setProps({ isLayouting: false, error: 'Network fetch failed' });
    expect(wrapper.text()).toContain('Network fetch failed');

    await wrapper.find('button').trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);

    await wrapper.setProps({ error: null });
    expect(wrapper.text()).toContain('No citation data');
  });

  it('hover_shows_node_tooltip_with_counts', async () => {
    const { wrapper } = await mountWithGraph();

    const enter = sigmaEvents.get('enterNode') as (p: { node: string }) => void;
    enter({ node: 'n1' });
    await nextTick();

    const text = wrapper.text();
    expect(text).toContain('Alpha Paper');
    expect(text).toContain('Foundations of the field');
    expect(text).toContain('5 cited');
    expect(text).toContain('3 refs');
  });

  it('isolation_mode_focuses_ancestry_or_progeny', async () => {
    const { wrapper, graph } = await mountWithGraph();

    /* Ancestry of n2 = transitive out-edges = {n3}; isolation set = {n2, n3};
     * n1 is dimmed (base color + 15% opacity suffix). */
    await wrapper.setProps({ isolationMode: { nodeId: 'n2', direction: 'ancestry' } });
    await nextTick();
    expect(graph.getNodeAttribute('n1', 'color')).toBe(`${citationClusterColor(0)}26`);
    expect(graph.getNodeAttribute('n2', 'color')).toBe(citationClusterColor(1));
    expect(graph.getNodeAttribute('n3', 'color')).toBe(citationClusterColor(2));

    /* Progeny of n2 = transitive in-edges = {n1}; isolation set = {n1, n2};
     * n3 is now the dimmed one. */
    await wrapper.setProps({ isolationMode: { nodeId: 'n2', direction: 'progeny' } });
    await nextTick();
    expect(graph.getNodeAttribute('n1', 'color')).toBe(citationClusterColor(0));
    expect(graph.getNodeAttribute('n2', 'color')).toBe(citationClusterColor(1));
    expect(graph.getNodeAttribute('n3', 'color')).toBe(`${citationClusterColor(2)}26`);
  });

  it('color_mode_switch_updates_node_colors', async () => {
    const { wrapper, graph } = await mountWithGraph();

    expect(graph.getNodeAttribute('n1', 'color')).toBe(citationClusterColor(0));
    expect(graph.getNodeAttribute('n2', 'color')).toBe(citationClusterColor(1));

    await wrapper.setProps({ colorMode: 'temporal' });
    await nextTick();

    expect(graph.getNodeAttribute('n1', 'color')).toBe(getTemporalColor(2018, 2018, 2022));
    expect(graph.getNodeAttribute('n2', 'color')).toBe(getTemporalColor(2020, 2018, 2022));
    expect(graph.getNodeAttribute('n1', 'color')).not.toBe(citationClusterColor(0));
  });
});
