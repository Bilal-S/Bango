import { describe, it, expect } from 'vitest';
import Graph from 'graphology';
import { applyFocusClusterVisualState, type VisualStateConfig } from '@/utils/network-visual-state';

function makeGraph(): Graph {
  const g = new Graph({ type: 'undirected' });
  g.addNode('a', { label: 'A', weight: 10, cluster: 0, size: 8 });
  g.addNode('b', { label: 'B', weight: 5, cluster: 0, size: 8 });
  g.addNode('c', { label: 'C', weight: 1, cluster: 1, size: 8 });
  g.addUndirectedEdge('a', 'b', { weight: 3 });
  g.addUndirectedEdge('b', 'c', { weight: 1 });
  return g;
}

const cfg: VisualStateConfig = {
  getNodeColor: (id) => (id === 'c' ? '#ff0000' : '#00ff00'),
  getNodeSize: (id, minW, maxW) => {
    const w = { a: 10, b: 5, c: 1 }[id] ?? 1;
    return minW === maxW ? 10 : 5 + ((w - minW) / (maxW - minW)) * 20;
  },
};

describe('applyFocusClusterVisualState', () => {
  it('clear state: base colors, weight-scaled sizes, idle edge color', () => {
    const g = makeGraph();
    applyFocusClusterVisualState(g, { focusedNodeId: null, selectedClusters: [] }, cfg);

    expect(g.getNodeAttribute('a', 'color')).toBe('#00ff00');
    expect(g.getNodeAttribute('c', 'color')).toBe('#ff0000');
    // weight 10 of range [1,10] -> 5 + (9/9)*20 = 25
    expect(g.getNodeAttribute('a', 'size')).toBe(25);
    // weight 1 (min) -> 5
    expect(g.getNodeAttribute('c', 'size')).toBe(5);
    expect(g.getEdgeAttribute(g.edge('a', 'b')!, 'color')).toBe('#cbd5e1');
  });

  it('focus mode: neighbors keep base color/size, others dim to alpha + 0.6x', () => {
    const g = makeGraph();
    // Focus 'a': neighbors {a,b}; 'c' is outside.
    applyFocusClusterVisualState(g, { focusedNodeId: 'a', selectedClusters: [] }, cfg);

    expect(g.getNodeAttribute('a', 'color')).toBe('#00ff00');
    expect(g.getNodeAttribute('b', 'color')).toBe('#00ff00');
    expect(g.getNodeAttribute('c', 'color')).toBe('#ff000026');
    const cSize = g.getNodeAttribute('c', 'size') as number;
    expect(cSize).toBeCloseTo(5 * 0.6);
    // Edge a-b highlighted; b-c dimmed.
    expect(g.getEdgeAttribute(g.edge('a', 'b')!, 'color')).toBe('#94a3b8');
    expect(g.getEdgeAttribute(g.edge('b', 'c')!, 'color')).toBe('#f1f5f9');
  });

  it('cluster mode: selected-cluster nodes stay bright, others dim', () => {
    const g = makeGraph();
    applyFocusClusterVisualState(g, { focusedNodeId: null, selectedClusters: [0] }, cfg);

    expect(g.getNodeAttribute('a', 'color')).toBe('#00ff00');
    expect(g.getNodeAttribute('c', 'color')).toBe('#ff000026');
    // In-cluster edge highlighted; cross-cluster edge dimmed.
    expect(g.getEdgeAttribute(g.edge('a', 'b')!, 'color')).toBe('#94a3b8');
    expect(g.getEdgeAttribute(g.edge('b', 'c')!, 'color')).toBe('#f1f5f9');
  });

  it('focus and cluster dim sets combine (union semantics, as keyword/cocitation)', () => {
    const g = makeGraph();
    // Focus 'c' (cluster 1) while cluster 1 is also selected: 'a'/'b' dim by
    // focus, 'c' stays bright (in the focus set AND the selected cluster).
    applyFocusClusterVisualState(g, { focusedNodeId: 'c', selectedClusters: [1] }, cfg);

    expect(g.getNodeAttribute('c', 'color')).toBe('#ff0000');
    expect(g.getNodeAttribute('a', 'color')).toBe('#00ff0026');
    expect(g.getNodeAttribute('b', 'color')).toBe('#00ff0026');
  });

  it('edge thickness pass applies scaled sizes (cocitation shape)', () => {
    const g = makeGraph();
    const cocitationCfg: VisualStateConfig = {
      getNodeColor: () => '#0000ff',
      getNodeSize: (_id) => 8,
      computeEdgeSize: (edge, minW, maxW) => {
        const w = g.getEdgeAttribute(edge, 'weight') as number;
        return minW === maxW ? 1.5 : 0.8 + ((w - minW) / (maxW - minW)) * (4 - 0.8);
      },
    };
    applyFocusClusterVisualState(g, { focusedNodeId: null, selectedClusters: [] }, cocitationCfg);

    // Edge weights [3,1] with bounds floored at 0 (cocitation's original
    // Math.min(...weights, 0)): ab at the top of [0.8, 4], bc at 1/3.
    expect(g.getEdgeAttribute(g.edge('a', 'b')!, 'size')).toBeCloseTo(4);
    expect(g.getEdgeAttribute(g.edge('b', 'c')!, 'size')).toBeCloseTo(0.8 + (1 / 3) * 3.2);
  });
});
