import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ref, nextTick } from 'vue';
import Graph from 'graphology';

// Mock the layout composable so onRecalculate runs deterministically.
// A counter makes each call produce distinct x/y so write-back is observable.
let layoutCounter = 0;
const mockApplyLayout = vi.fn(async (_g: Graph) => {
  // Simulate the real layout: assign distinct x/y + cluster to each node.
  _g.forEachNode((node, attrs) => {
    _g.setNodeAttribute(node, 'x', 10 + layoutCounter++);
    _g.setNodeAttribute(node, 'y', 20 + layoutCounter++);
    if (attrs.cluster === null || attrs.cluster === undefined) {
      _g.setNodeAttribute(node, 'cluster', 0);
    }
  });
});

vi.mock('@/composables/use-network-layout', () => ({
  useNetworkLayout: () => ({
    isLayouting: ref(false),
    applyLayout: mockApplyLayout,
    applyCircularLayout: vi.fn(),
    detectCommunities: vi.fn(),
    runForceAtlas2Async: vi.fn(),
  }),
}));

// Mock the export utilities so we can assert calls without Tauri/dialog.
vi.mock('@/utils/network-export', () => ({
  exportNetworkPng: vi.fn().mockResolvedValue(true),
  exportNetworkGexf: vi.fn().mockResolvedValue(true),
}));

import { exportNetworkPng, exportNetworkGexf } from '@/utils/network-export';
import { useNetworkView, type NetworkGraphHandle } from '@/composables/use-network-view';

// ─── Helpers ─────────────────────────────────────────────────────

/**
 * Build a small graph with the attributes the composable reads:
 * `label`, `cluster`, `year`/`avgYear`, `hidden`, `weight`.
 */
function makeGraph(
  nodes: Array<{
    id: string;
    label: string;
    cluster?: number | null;
    year?: number | null;
    avgYear?: number | null;
    hidden?: boolean;
    weight?: number;
  }>,
  edges: Array<{ source: string; target: string; weight?: number; hidden?: boolean }> = [],
  graphType: 'directed' | 'undirected' = 'undirected'
): Graph {
  const g = new Graph({ type: graphType, multi: false });
  for (const n of nodes) {
    g.addNode(n.id, {
      label: n.label,
      cluster: n.cluster ?? null,
      year: n.year ?? null,
      avgYear: n.avgYear ?? null,
      hidden: n.hidden ?? false,
      weight: n.weight ?? 1,
      x: 0,
      y: 0,
    });
  }
  for (const e of edges) {
    const add = graphType === 'directed' ? g.addDirectedEdge : g.addUndirectedEdge;
    add.call(g, e.source, e.target, {
      weight: e.weight ?? 1,
      hidden: e.hidden ?? false,
    });
  }
  return g;
}

/** A minimal NetworkGraphHandle stub for locate/reset/refresh assertions. */
function makeGraphHandle(overrides: Partial<NetworkGraphHandle> = {}): NetworkGraphHandle {
  return {
    locateNode: vi.fn(),
    resetZoom: vi.fn(),
    refresh: vi.fn(),
    ...overrides,
  };
}

// ─── clusterCount ────────────────────────────────────────────────

describe('clusterCount', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('counts distinct clusters among visible nodes', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'A', cluster: 0 },
        { id: 'b', label: 'B', cluster: 0 },
        { id: 'c', label: 'C', cluster: 1 },
        { id: 'd', label: 'D', cluster: 2 },
      ])
    );
    const { clusterCount } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(clusterCount.value).toBe(3);
  });

  it('excludes hidden nodes from the count', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'A', cluster: 0 },
        { id: 'b', label: 'B', cluster: 1, hidden: true },
        { id: 'c', label: 'C', cluster: 2, hidden: true },
      ])
    );
    const { clusterCount } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(clusterCount.value).toBe(1);
  });

  it('returns 0 when the graph is null', () => {
    const graph = ref<Graph | null>(null);
    const { clusterCount } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(clusterCount.value).toBe(0);
  });

  it('ignores nodes with null cluster', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'A', cluster: null },
        { id: 'b', label: 'B', cluster: 5 },
      ])
    );
    const { clusterCount } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(clusterCount.value).toBe(1);
  });
});

// ─── yearRange ───────────────────────────────────────────────────

describe('yearRange', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('computes min/max from the "year" attribute by default', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'A', year: 2018 },
        { id: 'b', label: 'B', year: 2023 },
        { id: 'c', label: 'C', year: 2020 },
      ])
    );
    const { yearRange } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(yearRange.value).toEqual({ min: 2018, max: 2023 });
  });

  it('reads "avgYear" when configured', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'A', avgYear: 2019.5 },
        { id: 'b', label: 'B', avgYear: 2021.5 },
      ])
    );
    const { yearRange } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
      yearAttribute: 'avgYear',
    });
    expect(yearRange.value).toEqual({ min: 2019, max: 2022 });
  });

  it('returns the default range when the graph is null', () => {
    const graph = ref<Graph | null>(null);
    const { yearRange } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
      defaultYearRange: { min: 2000, max: 2026 },
    });
    expect(yearRange.value).toEqual({ min: 2000, max: 2026 });
  });

  it('returns the default range when no nodes have years', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const { yearRange } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(yearRange.value).toEqual({ min: 2000, max: 2024 });
  });

  it('pads by 1 on each side when all nodes share the same year', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'A', year: 2020 },
        { id: 'b', label: 'B', year: 2020 },
      ])
    );
    const { yearRange } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(yearRange.value).toEqual({ min: 2019, max: 2021 });
  });
});

// ─── focusNode / navigateToNode / locateByLabel ──────────────────

describe('selection focus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('focusNode sets and clears focusedNodeId', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const { focusedNodeId, focusNode } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    expect(focusedNodeId.value).toBeNull();
    focusNode('a');
    expect(focusedNodeId.value).toBe('a');
    focusNode(null);
    expect(focusedNodeId.value).toBeNull();
  });

  it('navigateToNode focuses and calls locateNode on the graph handle', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const handle = makeGraphHandle();
    const { graphRef, navigateToNode, focusedNodeId } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    graphRef.value = handle;
    navigateToNode('a');
    expect(focusedNodeId.value).toBe('a');
    expect(handle.locateNode).toHaveBeenCalledWith('a');
  });

  it('locateByLabel finds, focuses, and locates a node by label', () => {
    const graph = ref<Graph | null>(
      makeGraph([
        { id: 'a', label: 'Alice' },
        { id: 'b', label: 'Bob' },
      ])
    );
    const handle = makeGraphHandle();
    const { graphRef, locateByLabel, focusedNodeId } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    graphRef.value = handle;
    const found = locateByLabel('Bob');
    expect(found).toBe('b');
    expect(focusedNodeId.value).toBe('b');
    expect(handle.locateNode).toHaveBeenCalledWith('b');
  });

  it('locateByLabel returns null when no node matches', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'Alice' }]));
    const { locateByLabel, focusedNodeId } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    const found = locateByLabel('Nobody');
    expect(found).toBeNull();
    expect(focusedNodeId.value).toBeNull();
  });

  it('locateByLabel returns null when the graph is null', () => {
    const graph = ref<Graph | null>(null);
    const { locateByLabel } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    expect(locateByLabel('Alice')).toBeNull();
  });
});

// ─── onSelectCluster / onClearClusters ───────────────────────────

describe('cluster selection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('onSelectCluster toggles a cluster id in', () => {
    const graph = ref<Graph | null>(makeGraph([]));
    const { selectedClusters, onSelectCluster } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    expect(selectedClusters.value).toEqual([]);
    onSelectCluster(1);
    expect(selectedClusters.value).toEqual([1]);
    onSelectCluster(3);
    expect(selectedClusters.value).toEqual([1, 3]);
  });

  it('onSelectCluster toggles a cluster id out', () => {
    const graph = ref<Graph | null>(makeGraph([]));
    const { selectedClusters, onSelectCluster } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    onSelectCluster(1);
    onSelectCluster(3);
    onSelectCluster(1); // remove
    expect(selectedClusters.value).toEqual([3]);
  });

  it('onClearClusters empties the selected set', () => {
    const graph = ref<Graph | null>(makeGraph([]));
    const { selectedClusters, onSelectCluster, onClearClusters } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    onSelectCluster(1);
    onSelectCluster(2);
    onClearClusters();
    expect(selectedClusters.value).toEqual([]);
  });
});

// ─── onExportImage ───────────────────────────────────────────────

describe('onExportImage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls exportNetworkPng with the configured prefix when format is png', async () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const fakeRenderer = {} as never;
    const { onExportImage } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'coauthor-network',
    });
    await onExportImage('png', fakeRenderer);
    expect(exportNetworkPng).toHaveBeenCalledWith(fakeRenderer, 'coauthor-network.png');
  });

  it('calls exportNetworkGexf with the configured prefix when format is gexf', async () => {
    const g = makeGraph([{ id: 'a', label: 'A' }]);
    const graph = ref<Graph | null>(g);
    const { onExportImage } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'keyword-network',
    });
    await onExportImage('gexf', null);
    expect(exportNetworkGexf).toHaveBeenCalledWith(g, 'keyword-network.gexf');
  });

  it('early-returns when format is png but no renderer is passed', async () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const { onExportImage } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    await onExportImage('png', null);
    expect(exportNetworkPng).not.toHaveBeenCalled();
  });

  it('early-returns when format is gexf but graph is null', async () => {
    const graph = ref<Graph | null>(null);
    const { onExportImage } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    await onExportImage('gexf', null);
    expect(exportNetworkGexf).not.toHaveBeenCalled();
  });

  it('logs an error and swallows it when export throws', async () => {
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.mocked(exportNetworkPng).mockRejectedValueOnce(new Error('boom'));
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const { onExportImage } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    await onExportImage('png', {} as never);
    expect(errSpy).toHaveBeenCalledWith(expect.stringContaining('test'), expect.any(Error));
    errSpy.mockRestore();
  });
});

// ─── onRecalculate ───────────────────────────────────────────────

describe('onRecalculate', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApplyLayout.mockClear();
    layoutCounter = 0;
  });

  it('builds an undirected subgraph from visible nodes only', async () => {
    const g = makeGraph(
      [
        { id: 'a', label: 'A', cluster: null },
        { id: 'b', label: 'B', cluster: null },
        { id: 'c', label: 'C', hidden: true, cluster: null },
      ],
      [
        { source: 'a', target: 'b' },
        { source: 'a', target: 'c' },
      ]
    );
    const graph = ref<Graph | null>(g);
    const handle = makeGraphHandle();
    const { graphRef, onRecalculate, recalculateTrigger } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
      graphType: 'undirected',
    });
    graphRef.value = handle;
    const before = recalculateTrigger.value;
    await onRecalculate();
    // applyLayout receives the subgraph (2 visible nodes, 1 visible edge).
    expect(mockApplyLayout).toHaveBeenCalledTimes(1);
    const sub = mockApplyLayout.mock.calls[0]![0] as Graph;
    expect(sub.order).toBe(2);
    expect(sub.size).toBe(1);
    // Trigger bumped.
    expect(recalculateTrigger.value).toBe(before + 1);
    // resetZoom + refresh called.
    expect(handle.resetZoom).toHaveBeenCalled();
    expect(handle.refresh).toHaveBeenCalled();
  });

  it('builds a directed subgraph when graphType is "directed"', async () => {
    const g = makeGraph(
      [
        { id: 'a', label: 'A', cluster: null },
        { id: 'b', label: 'B', cluster: null },
      ],
      [{ source: 'a', target: 'b' }],
      'directed'
    );
    const graph = ref<Graph | null>(g);
    const { onRecalculate } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
      graphType: 'directed',
    });
    await onRecalculate();
    const sub = mockApplyLayout.mock.calls[0]![0] as Graph;
    expect(sub.type).toBe('directed');
  });

  it('writes back x, y, and cluster attributes to the parent graph', async () => {
    const g = makeGraph([
      { id: 'a', label: 'A', cluster: null },
      { id: 'b', label: 'B', cluster: null },
    ]);
    const graph = ref<Graph | null>(g);
    const { onRecalculate } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    // Capture original values (seed x=0, cluster=null).
    expect(g.getNodeAttribute('a', 'x')).toBe(0);
    expect(g.getNodeAttribute('a', 'cluster')).toBeNull();
    // mockApplyLayout sets x/y to distinct non-zero values + cluster to 0.
    await onRecalculate();
    expect(g.getNodeAttribute('a', 'x')).not.toBe(0);
    expect(g.getNodeAttribute('a', 'y')).not.toBe(0);
    expect(g.getNodeAttribute('a', 'cluster')).toBe(0);
    expect(g.getNodeAttribute('b', 'cluster')).toBe(0);
  });

  it('no-ops when the graph is null', async () => {
    const graph = ref<Graph | null>(null);
    const { onRecalculate, recalculateTrigger } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    const before = recalculateTrigger.value;
    await onRecalculate();
    expect(mockApplyLayout).not.toHaveBeenCalled();
    expect(recalculateTrigger.value).toBe(before);
  });

  it('no-ops when the subgraph has zero visible nodes', async () => {
    const g = makeGraph([{ id: 'a', label: 'A', hidden: true }]);
    const graph = ref<Graph | null>(g);
    const { onRecalculate, recalculateTrigger } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    const before = recalculateTrigger.value;
    await onRecalculate();
    expect(mockApplyLayout).not.toHaveBeenCalled();
    expect(recalculateTrigger.value).toBe(before);
  });

  it('sets isLayouting to true during the run and false after', async () => {
    const g = makeGraph([{ id: 'a', label: 'A', cluster: null }]);
    const graph = ref<Graph | null>(g);
    const { isLayouting, onRecalculate } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    expect(isLayouting.value).toBe(false);
    const promise = onRecalculate();
    expect(isLayouting.value).toBe(true);
    await promise;
    expect(isLayouting.value).toBe(false);
  });
});

// ─── onLayoutModeChange ──────────────────────────────────────────

describe('onLayoutModeChange', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApplyLayout.mockClear();
  });

  it('updates layoutMode and triggers a recalculate', async () => {
    const g = makeGraph([{ id: 'a', label: 'A', cluster: null }]);
    const graph = ref<Graph | null>(g);
    const { layoutMode, onLayoutModeChange } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    expect(layoutMode.value).toBe('fixed');
    await onLayoutModeChange('dynamic');
    expect(layoutMode.value).toBe('dynamic');
    expect(mockApplyLayout).toHaveBeenCalled();
  });

  it('does not recalculate when the graph is null', async () => {
    const graph = ref<Graph | null>(null);
    const { layoutMode, onLayoutModeChange } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    await onLayoutModeChange('dynamic');
    expect(layoutMode.value).toBe('dynamic');
    expect(mockApplyLayout).not.toHaveBeenCalled();
  });
});

// ─── resetViewState ──────────────────────────────────────────────

describe('resetViewState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('resets colorMode, layoutMode, selectedClusters, and focusedNodeId', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const {
      colorMode,
      layoutMode,
      selectedClusters,
      focusedNodeId,
      resetViewState,
      onSelectCluster,
      focusNode,
    } = useNetworkView({ graph: graph as never, exportPrefix: 'test' });
    // Mutate first.
    colorMode.value = 'temporal';
    layoutMode.value = 'dynamic';
    onSelectCluster(2);
    focusNode('a');
    // Reset.
    resetViewState();
    expect(colorMode.value).toBe('cluster');
    expect(layoutMode.value).toBe('fixed');
    expect(selectedClusters.value).toEqual([]);
    expect(focusedNodeId.value).toBeNull();
  });
});

// ─── graph handle proxies ────────────────────────────────────────

describe('graph handle proxies', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('locateNode, resetZoom, refresh forward to the graph handle', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const handle = makeGraphHandle();
    const { graphRef, locateNode, resetZoom, refresh } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    graphRef.value = handle;
    locateNode('a');
    resetZoom();
    refresh();
    expect(handle.locateNode).toHaveBeenCalledWith('a');
    expect(handle.resetZoom).toHaveBeenCalled();
    expect(handle.refresh).toHaveBeenCalled();
  });

  it('proxies are no-ops when graphRef is null', () => {
    const graph = ref<Graph | null>(makeGraph([{ id: 'a', label: 'A' }]));
    const { locateNode, resetZoom, refresh } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    expect(() => {
      locateNode('a');
      resetZoom();
      refresh();
    }).not.toThrow();
  });
});

// ─── reactivity ──────────────────────────────────────────────────

describe('reactivity', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('clusterCount updates when a node is hidden after recalculate', async () => {
    const g = makeGraph([
      { id: 'a', label: 'A', cluster: 0 },
      { id: 'b', label: 'B', cluster: 1 },
    ]);
    const graph = ref<Graph | null>(g);
    const handle = makeGraphHandle();
    const { graphRef, clusterCount, recalculateTrigger } = useNetworkView({
      graph: graph as never,
      exportPrefix: 'test',
    });
    graphRef.value = handle;
    expect(clusterCount.value).toBe(2);
    // Hide one node and bump the trigger.
    g.setNodeAttribute('b', 'hidden', true);
    recalculateTrigger.value++;
    await nextTick();
    expect(clusterCount.value).toBe(1);
  });
});
