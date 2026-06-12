import { describe, it, expect, vi, beforeEach } from 'vitest';
import Graph from 'graphology';
import { clusterColor, type NetworkData } from '@/types/biblio-network';

// ─── clusterColor ────────────────────────────────────────────────

describe('clusterColor', () => {
  it('returns the first palette color for cluster 0', () => {
    expect(clusterColor(0)).toBe('#E69F00'); // Okabe-Ito orange
  });

  it('returns the second palette color for cluster 1', () => {
    expect(clusterColor(1)).toBe('#56B4E9'); // sky blue
  });

  it('cycles through all 8 palette colors', () => {
    const colors = Array.from({ length: 8 }, (_, i) => clusterColor(i));
    const unique = new Set(colors);
    expect(unique.size).toBe(8);
  });

  it('wraps around for clusters > 7', () => {
    expect(clusterColor(8)).toBe(clusterColor(0));
    expect(clusterColor(9)).toBe(clusterColor(1));
    expect(clusterColor(16)).toBe(clusterColor(0));
  });

  it('handles cluster index 0 through 7', () => {
    for (let i = 0; i < 8; i++) {
      const color = clusterColor(i);
      expect(color).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });
});

// ─── Graph building (integration) ────────────────────────────────

// Mock tauri command
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import { useCoAuthorNetwork } from '@/composables/use-coauthor-network';

function makeNetworkData(overrides: Partial<NetworkData> = {}): NetworkData {
  return {
    nodes: [
      {
        id: 'a1',
        label: 'Alice Smith',
        weight: 5,
        totalCitations: 42,
        avgYear: 2020.5,
        estimatedHIndex: 3,
        cluster: null,
      },
      {
        id: 'a2',
        label: 'Bob Jones',
        weight: 3,
        totalCitations: 15,
        avgYear: 2019.0,
        estimatedHIndex: 2,
        cluster: null,
      },
      {
        id: 'a3',
        label: 'Carol White',
        weight: 1,
        totalCitations: 2,
        avgYear: 2022.0,
        estimatedHIndex: 1,
        cluster: null,
      },
    ],
    edges: [
      { source: 'a1', target: 'a2', weight: 3, fractionalWeight: 1.5 },
      { source: 'a1', target: 'a3', weight: 1, fractionalWeight: 0.5 },
    ],
    ...overrides,
  };
}

describe('useCoAuthorNetwork', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('fetches network data and builds a graph', async () => {
    const data = makeNetworkData();
    vi.mocked(tauriCommand).mockResolvedValue(data);

    const { fetchNetwork, graph, nodeCount, edgeCount } = useCoAuthorNetwork();
    await fetchNetwork();

    expect(tauriCommand).toHaveBeenCalledWith('biblio_get_coauthor_network');
    expect(graph.value).not.toBeNull();
    expect(nodeCount.value).toBe(3);
    expect(edgeCount.value).toBe(2);
  });

  it('scales node sizes based on weight', async () => {
    const data = makeNetworkData();
    vi.mocked(tauriCommand).mockResolvedValue(data);

    const { fetchNetwork, graph } = useCoAuthorNetwork();
    await fetchNetwork();

    const g = graph.value!;
    // Alice has weight=5 (max), should be larger than Carol with weight=1 (min)
    const aliceSize = g.getNodeAttribute('a1', 'size') as number;
    const carolSize = g.getNodeAttribute('a3', 'size') as number;
    expect(aliceSize).toBeGreaterThan(carolSize);
  });

  it('scales edge thickness based on weight', async () => {
    const data = makeNetworkData();
    vi.mocked(tauriCommand).mockResolvedValue(data);

    const { fetchNetwork, graph } = useCoAuthorNetwork();
    await fetchNetwork();

    const g = graph.value!;
    const edge = g.edge('a1', 'a2')!;
    const thinEdge = g.edge('a1', 'a3')!;
    const thickWeight = g.getEdgeAttribute(edge, 'thickness') as number;
    const thinWeight = g.getEdgeAttribute(thinEdge, 'thickness') as number;
    expect(thickWeight).toBeGreaterThan(thinWeight);
  });

  it('sets node attributes from API data', async () => {
    const data = makeNetworkData();
    vi.mocked(tauriCommand).mockResolvedValue(data);

    const { fetchNetwork, graph } = useCoAuthorNetwork();
    await fetchNetwork();

    const g = graph.value!;
    expect(g.getNodeAttribute('a1', 'label')).toBe('Alice Smith');
    expect(g.getNodeAttribute('a1', 'weight')).toBe(5);
    expect(g.getNodeAttribute('a1', 'totalCitations')).toBe(42);
    expect(g.getNodeAttribute('a1', 'avgYear')).toBe(2020.5);
    expect(g.getNodeAttribute('a1', 'estimatedHIndex')).toBe(3);
  });

  it('sets graph to null when no data returned', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(null);

    const { fetchNetwork, graph } = useCoAuthorNetwork();
    await fetchNetwork();

    expect(graph.value).toBeNull();
  });

  it('sets graph to null when nodes array is empty', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData({ nodes: [], edges: [] }));

    const { fetchNetwork, graph } = useCoAuthorNetwork();
    await fetchNetwork();

    expect(graph.value).toBeNull();
  });

  it('handles errors gracefully', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('IPC failed'));

    const { fetchNetwork, graph, error } = useCoAuthorNetwork();
    await fetchNetwork();

    expect(graph.value).toBeNull();
    expect(error.value).toBe('IPC failed');
  });

  it('clearGraph resets state', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, clearGraph, graph, error } = useCoAuthorNetwork();
    await fetchNetwork();
    expect(graph.value).not.toBeNull();

    clearGraph();
    expect(graph.value).toBeNull();
    expect(error.value).toBeNull();
  });

  it('skips edges with unknown nodes', async () => {
    const data = makeNetworkData({
      edges: [
        { source: 'a1', target: 'unknown', weight: 1, fractionalWeight: 0.5 },
        { source: 'a1', target: 'a2', weight: 2, fractionalWeight: 1.0 },
      ],
    });
    vi.mocked(tauriCommand).mockResolvedValue(data);

    const { fetchNetwork, edgeCount } = useCoAuthorNetwork();
    await fetchNetwork();

    // Only the a1-a2 edge should be added; a1-unknown is skipped
    expect(edgeCount.value).toBe(1);
  });

  it('handles loading state correctly', async () => {
    let resolve!: (v: unknown) => void;
    const promise = new Promise((r) => (resolve = r));
    vi.mocked(tauriCommand).mockReturnValue(promise);

    const { fetchNetwork, loading } = useCoAuthorNetwork();

    const call = fetchNetwork();
    expect(loading.value).toBe(true);

    resolve(makeNetworkData());
    await call;

    expect(loading.value).toBe(false);
  });
});

// ─── Layout integration ──────────────────────────────────────────

import { applyCircularLayout, detectCommunities } from '@/composables/use-network-layout';

describe('network layout utilities', () => {
  describe('applyCircularLayout', () => {
    it('positions all nodes in a circle', () => {
      const g = new Graph({ type: 'undirected' });
      for (let i = 0; i < 5; i++) {
        g.addNode(`n${i}`, { x: 0, y: 0, size: 5, label: `Node ${i}` });
      }

      applyCircularLayout(g, 100);

      // Each node should have unique x,y (not all at origin)
      const positions = g.nodes().map((n) => ({
        x: g.getNodeAttribute(n, 'x') as number,
        y: g.getNodeAttribute(n, 'y') as number,
      }));
      const uniquePositions = new Set(positions.map((p) => `${p.x},${p.y}`));
      expect(uniquePositions.size).toBe(5);

      // All nodes should be roughly at distance 100 from center
      for (const pos of positions) {
        const dist = Math.sqrt(pos.x ** 2 + pos.y ** 2);
        expect(dist).toBeCloseTo(100, 0);
      }
    });

    it('handles single node without division by zero', () => {
      const g = new Graph({ type: 'undirected' });
      g.addNode('only', { x: 0, y: 0, size: 5, label: 'Only' });

      applyCircularLayout(g, 50);
      // Should not throw
      expect(g.getNodeAttribute('only', 'x')).toBeDefined();
      expect(g.getNodeAttribute('only', 'y')).toBeDefined();
    });
  });

  describe('detectCommunities', () => {
    it('detects communities in a connected graph', () => {
      const g = new Graph({ type: 'undirected' });
      // Create a simple 3-node clique
      g.addNode('a', { x: 0, y: 0, size: 5, label: 'A', color: '#ccc', cluster: null });
      g.addNode('b', { x: 1, y: 0, size: 5, label: 'B', color: '#ccc', cluster: null });
      g.addNode('c', { x: 0, y: 1, size: 5, label: 'C', color: '#ccc', cluster: null });
      g.addUndirectedEdge('a', 'b', { weight: 1 });
      g.addUndirectedEdge('b', 'c', { weight: 1 });
      g.addUndirectedEdge('a', 'c', { weight: 1 });

      const count = detectCommunities(g);

      expect(count).toBeGreaterThanOrEqual(1);
      // All nodes should have a cluster assigned
      g.forEachNode((node) => {
        const cluster = g.getNodeAttribute(node, 'cluster');
        expect(cluster).not.toBeNull();
      });
      // All nodes should have a color
      g.forEachNode((node) => {
        const color = g.getNodeAttribute(node, 'color');
        expect(color).toMatch(/^#[0-9A-Fa-f]{6}$/);
      });
    });

    it('assigns different clusters to disconnected components', () => {
      const g = new Graph({ type: 'undirected' });
      // Two separate components
      g.addNode('a', { x: 0, y: 0, size: 5, label: 'A', color: '#ccc', cluster: null });
      g.addNode('b', { x: 1, y: 0, size: 5, label: 'B', color: '#ccc', cluster: null });
      g.addNode('c', { x: 10, y: 10, size: 5, label: 'C', color: '#ccc', cluster: null });
      g.addNode('d', { x: 11, y: 10, size: 5, label: 'D', color: '#ccc', cluster: null });
      g.addUndirectedEdge('a', 'b', { weight: 1 });
      g.addUndirectedEdge('c', 'd', { weight: 1 });

      const count = detectCommunities(g);

      expect(count).toBeGreaterThanOrEqual(2);
      // A and B should be in the same cluster
      expect(g.getNodeAttribute('a', 'cluster')).toBe(g.getNodeAttribute('b', 'cluster'));
      // C and D should be in the same cluster
      expect(g.getNodeAttribute('c', 'cluster')).toBe(g.getNodeAttribute('d', 'cluster'));
      // The two groups should be in different clusters
      expect(g.getNodeAttribute('a', 'cluster')).not.toBe(g.getNodeAttribute('c', 'cluster'));
    });
  });
});

// ─── Counting mode switching ─────────────────────────────────────

describe('setCountingMode', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('stores both full and fractional weights on edges', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, graph } = useCoAuthorNetwork();
    await fetchNetwork();

    const g = graph.value!;
    const eid = g.edge('a1', 'a2')!;
    expect(g.getEdgeAttribute(eid, 'fullWeight')).toBe(3);
    expect(g.getEdgeAttribute(eid, 'fractionalWeight')).toBe(1.5);
    // Default active weight should be full
    expect(g.getEdgeAttribute(eid, 'weight')).toBe(3);
  });

  it('defaults to full counting mode', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, countingMode } = useCoAuthorNetwork();
    await fetchNetwork();

    expect(countingMode.value).toBe('full');
  });

  it('switches edge weights to fractional on mode change', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, graph, setCountingMode, countingMode } = useCoAuthorNetwork();
    await fetchNetwork();

    const changed = setCountingMode('fractional');
    expect(changed).toBe(true);
    expect(countingMode.value).toBe('fractional');

    const g = graph.value!;
    const eid = g.edge('a1', 'a2')!;
    expect(g.getEdgeAttribute(eid, 'weight')).toBe(1.5);
  });

  it('switches back to full counting mode', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, graph, setCountingMode } = useCoAuthorNetwork();
    await fetchNetwork();

    setCountingMode('fractional');
    setCountingMode('full');

    const g = graph.value!;
    const eid = g.edge('a1', 'a2')!;
    expect(g.getEdgeAttribute(eid, 'weight')).toBe(3);
  });

  it('returns false when switching to same mode', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, setCountingMode } = useCoAuthorNetwork();
    await fetchNetwork();

    const changed = setCountingMode('full');
    expect(changed).toBe(false);
  });

  it('resets counting mode on clearGraph', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, setCountingMode, clearGraph, countingMode } = useCoAuthorNetwork();
    await fetchNetwork();
    setCountingMode('fractional');
    expect(countingMode.value).toBe('fractional');

    clearGraph();
    expect(countingMode.value).toBe('full');
  });

  it('rescales edge thickness after mode switch', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(makeNetworkData());

    const { fetchNetwork, graph, setCountingMode } = useCoAuthorNetwork();
    await fetchNetwork();

    const g = graph.value!;
    // Check the thinner edge (a1-a3) — it's the min in both ranges
    // but the ratio differs: full 1/(3-1)=0.5 vs fractional 0.5/(1.5-0.5)=0.5
    // Both map to mid-range when ratio is same. Instead, check the thick edge's
    // fractional weight is now active.
    const eid = g.edge('a1', 'a2')!;
    expect(g.getEdgeAttribute(eid, 'weight')).toBe(3); // full mode

    setCountingMode('fractional');
    expect(g.getEdgeAttribute(eid, 'weight')).toBe(1.5); // now fractional

    // Thickness is recalculated — should still be within valid output range [0.5, 4]
    const thickness = g.getEdgeAttribute(eid, 'thickness') as number;
    expect(thickness).toBeGreaterThanOrEqual(0.5);
    expect(thickness).toBeLessThanOrEqual(4);
  });
});
