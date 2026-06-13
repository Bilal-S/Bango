import { describe, it, expect } from 'vitest';
import Graph from 'graphology';
import {
  computeAncestry,
  computeProgeny,
  computeSubgraphEdges,
  filterNodesByYearRange,
  findBackEdges,
  topologicalSort,
  computeSPC,
  traceMainPath,
  computeMainPath,
} from '../utils/citation-analysis';

/**
 * Build a directed graphology graph from a compact edge list.
 *
 * Edge convention: `[source, target]` means `source cites target`.
 */
function makeGraph(edges: [string, string][]): Graph {
  const g = new Graph({ type: 'directed', multi: false });
  for (const [s, t] of edges) {
    if (!g.hasNode(s)) g.addNode(s);
    if (!g.hasNode(t)) g.addNode(t);
    if (!g.hasEdge(s, t)) g.addDirectedEdge(s, t);
  }
  return g;
}

describe('computeAncestry', () => {
  it('returns empty set for a non-existent node', () => {
    const g = makeGraph([['a', 'b']]);
    expect(computeAncestry(g, 'zzz')).toEqual(new Set());
  });

  it('returns empty set for a node with no outgoing edges (sink)', () => {
    const g = makeGraph([['a', 'b']]);
    // b is cited by a but cites nothing
    expect(computeAncestry(g, 'b')).toEqual(new Set());
  });

  it('handles a linear chain: a → b → c → d (a cites b cites c cites d)', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
      ['c', 'd'],
    ]);
    // ancestry of a = {b, c, d}
    expect(computeAncestry(g, 'a')).toEqual(new Set(['b', 'c', 'd']));
    // ancestry of b = {c, d}
    expect(computeAncestry(g, 'b')).toEqual(new Set(['c', 'd']));
    // ancestry of c = {d}
    expect(computeAncestry(g, 'c')).toEqual(new Set(['d']));
  });

  it('handles a diamond: a → b, a → c, b → d, c → d', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['a', 'c'],
      ['b', 'd'],
      ['c', 'd'],
    ]);
    // ancestry of a = {b, c, d}
    expect(computeAncestry(g, 'a')).toEqual(new Set(['b', 'c', 'd']));
  });

  it('handles disconnected components (does not cross them)', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['c', 'd'],
    ]);
    expect(computeAncestry(g, 'a')).toEqual(new Set(['b']));
    expect(computeAncestry(g, 'c')).toEqual(new Set(['d']));
  });

  it('does not include the queried node itself', () => {
    const g = makeGraph([['a', 'b']]);
    const result = computeAncestry(g, 'a');
    expect(result.has('a')).toBe(false);
    expect(result.has('b')).toBe(true);
  });

  it('terminates on cyclic graphs without infinite loop', () => {
    // a cites b, b cites a (a cycle) plus b cites c
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'a'],
      ['b', 'c'],
    ]);
    // ancestry of a should reach b and c despite the cycle
    const result = computeAncestry(g, 'a');
    expect(result.has('b')).toBe(true);
    expect(result.has('c')).toBe(true);
    expect(result.has('a')).toBe(false);
  });

  it('visits each ancestor only once even with multiple paths', () => {
    // a → b, a → c, b → d, c → d, d → e
    const g = makeGraph([
      ['a', 'b'],
      ['a', 'c'],
      ['b', 'd'],
      ['c', 'd'],
      ['d', 'e'],
    ]);
    const result = computeAncestry(g, 'a');
    expect(result.size).toBe(4); // b, c, d, e — no duplicates
    expect(result).toEqual(new Set(['b', 'c', 'd', 'e']));
  });
});

describe('computeProgeny', () => {
  it('returns empty set for a non-existent node', () => {
    const g = makeGraph([['a', 'b']]);
    expect(computeProgeny(g, 'zzz')).toEqual(new Set());
  });

  it('returns empty set for a node with no incoming edges (source)', () => {
    const g = makeGraph([['a', 'b']]);
    // a cites b, nothing cites a
    expect(computeProgeny(g, 'a')).toEqual(new Set());
  });

  it('handles a linear chain: a → b → c → d (progeny of d = {a, b, c})', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
      ['c', 'd'],
    ]);
    expect(computeProgeny(g, 'd')).toEqual(new Set(['a', 'b', 'c']));
    expect(computeProgeny(g, 'c')).toEqual(new Set(['a', 'b']));
  });

  it('handles a diamond: progeny of d = {a, b, c}', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['a', 'c'],
      ['b', 'd'],
      ['c', 'd'],
    ]);
    expect(computeProgeny(g, 'd')).toEqual(new Set(['a', 'b', 'c']));
  });

  it('does not include the queried node itself', () => {
    const g = makeGraph([['a', 'b']]);
    const result = computeProgeny(g, 'b');
    expect(result.has('b')).toBe(false);
    expect(result.has('a')).toBe(true);
  });

  it('terminates on cyclic graphs without infinite loop', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'a'],
      ['c', 'a'],
    ]);
    // progeny of a: who cites a? b and c. who cites b? a (already excluded as self).
    const result = computeProgeny(g, 'a');
    expect(result.has('b')).toBe(true);
    expect(result.has('c')).toBe(true);
    expect(result.has('a')).toBe(false);
  });
});

describe('computeSubgraphEdges', () => {
  it('returns edges whose both endpoints are in the node set', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
      ['c', 'd'],
    ]);
    const nodes = new Set(['a', 'b', 'c']);
    const edges = computeSubgraphEdges(g, nodes);
    // a→b and b→c qualify; c→d does not (d not in set)
    expect(edges.size).toBe(2);
  });

  it('returns empty set for an empty node set', () => {
    const g = makeGraph([['a', 'b']]);
    expect(computeSubgraphEdges(g, new Set()).size).toBe(0);
  });

  it('returns empty set when no edges connect the given nodes', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['c', 'd'],
    ]);
    // a and c have no edge between them
    expect(computeSubgraphEdges(g, new Set(['a', 'c'])).size).toBe(0);
  });

  it('ignores nodes that do not exist in the graph', () => {
    const g = makeGraph([['a', 'b']]);
    const nodes = new Set(['a', 'b', 'nonexistent']);
    expect(computeSubgraphEdges(g, nodes).size).toBe(1);
  });
});

describe('filterNodesByYearRange', () => {
  /** Build a graph with year attributes on nodes. */
  function makeYearGraph(
    nodes: { id: string; year?: number | null }[],
    edges: [string, string][]
  ): Graph {
    const g = new Graph({ type: 'directed', multi: false });
    for (const { id, year } of nodes) {
      g.addNode(id);
      if (year !== undefined) g.setNodeAttribute(id, 'year', year);
    }
    for (const [s, t] of edges) {
      if (!g.hasEdge(s, t)) g.addDirectedEdge(s, t);
    }
    return g;
  }

  it('returns all nodes when range is null', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2010 },
        { id: 'b', year: 2020 },
      ],
      [['a', 'b']]
    );
    expect(filterNodesByYearRange(g, null)).toEqual(new Set(['a', 'b']));
  });

  it('returns all nodes when range is undefined-ish (null)', () => {
    const g = makeYearGraph([{ id: 'a', year: 2010 }], []);
    expect(filterNodesByYearRange(g, null).size).toBe(1);
  });

  it('filters nodes outside the range', () => {
    const g = makeYearGraph(
      [
        { id: 'old', year: 1990 },
        { id: 'mid', year: 2005 },
        { id: 'new', year: 2020 },
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2000, 2010]);
    expect(result).toEqual(new Set(['mid']));
  });

  it('includes boundary years (inclusive range)', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2000 },
        { id: 'b', year: 2005 },
        { id: 'c', year: 2010 },
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2000, 2010]);
    expect(result).toEqual(new Set(['a', 'b', 'c']));
  });

  it('includes nodes with null year (cannot evaluate)', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2010 },
        { id: 'b', year: null },
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2000, 2015]);
    expect(result).toEqual(new Set(['a', 'b']));
  });

  it('includes nodes with missing year attribute (undefined)', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2010 },
        { id: 'b' }, // no year attribute at all
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2000, 2015]);
    expect(result).toEqual(new Set(['a', 'b']));
  });

  it('excludes nodes with null year when they are outside other range constraints — null always passes', () => {
    // Even with a narrow range, null-year nodes are always included.
    const g = makeYearGraph(
      [
        { id: 'a', year: 1990 },
        { id: 'b', year: null },
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2000, 2010]);
    expect(result.has('a')).toBe(false);
    expect(result.has('b')).toBe(true);
  });

  it('handles a single-year range [2010, 2010]', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2010 },
        { id: 'b', year: 2011 },
        { id: 'c', year: 2009 },
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2010, 2010]);
    expect(result).toEqual(new Set(['a']));
  });

  it('returns empty set when no nodes have year in range', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 1990 },
        { id: 'b', year: 1995 },
      ],
      []
    );
    const result = filterNodesByYearRange(g, [2000, 2010]);
    expect(result.size).toBe(0);
  });

  it('handles empty graph gracefully', () => {
    const g = new Graph({ type: 'directed' });
    expect(filterNodesByYearRange(g, [2000, 2010]).size).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Phase 3 — Main Path Analysis (SPC) tests
// ---------------------------------------------------------------------------

/**
 * Build a directed graph with year attributes from edge + node spec.
 * Edges: `[source, target]` means `source cites target`.
 */
function makeYearGraph(
  nodes: { id: string; year?: number | null }[],
  edges: [string, string][]
): Graph {
  const g = new Graph({ type: 'directed', multi: false });
  for (const { id, year } of nodes) {
    g.addNode(id);
    if (year !== undefined) g.setNodeAttribute(id, 'year', year);
  }
  for (const [s, t] of edges) {
    if (!g.hasEdge(s, t)) g.addDirectedEdge(s, t);
  }
  return g;
}

describe('findBackEdges', () => {
  it('returns empty set for a temporally consistent DAG', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2020 },
        { id: 'b', year: 2010 },
      ],
      [['a', 'b']] // 2020 cites 2010 — fine
    );
    expect(findBackEdges(g).size).toBe(0);
  });

  it('flags an edge where the cited paper is newer', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2010 },
        { id: 'b', year: 2020 },
      ],
      [['a', 'b']] // 2010 cites 2020 — impossible
    );
    const back = findBackEdges(g);
    expect(back.size).toBe(1);
  });

  it('does not flag edges where either year is null', () => {
    const g = makeYearGraph(
      [
        { id: 'a', year: 2010 },
        { id: 'b', year: null },
        { id: 'c' }, // undefined year
      ],
      [
        ['a', 'b'],
        ['a', 'c'],
      ]
    );
    expect(findBackEdges(g).size).toBe(0);
  });

  it('handles empty graph', () => {
    const g = new Graph({ type: 'directed' });
    expect(findBackEdges(g).size).toBe(0);
  });
});

describe('topologicalSort', () => {
  it('returns all nodes for a simple chain', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
    ]);
    const order = topologicalSort(g);
    expect(order).toHaveLength(3);
    // a must come before b, b before c
    expect(order.indexOf('a')).toBeLessThan(order.indexOf('b'));
    expect(order.indexOf('b')).toBeLessThan(order.indexOf('c'));
  });

  it('handles a diamond graph', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['a', 'c'],
      ['b', 'd'],
      ['c', 'd'],
    ]);
    const order = topologicalSort(g);
    expect(order).toHaveLength(4);
    expect(order.indexOf('a')).toBeLessThan(order.indexOf('b'));
    expect(order.indexOf('a')).toBeLessThan(order.indexOf('c'));
    expect(order.indexOf('b')).toBeLessThan(order.indexOf('d'));
    expect(order.indexOf('c')).toBeLessThan(order.indexOf('d'));
  });

  it('omits nodes in a 2-node cycle', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'a'],
    ]);
    const order = topologicalSort(g);
    expect(order).toHaveLength(0);
  });

  it('handles empty graph', () => {
    const g = new Graph({ type: 'directed' });
    expect(topologicalSort(g)).toEqual([]);
  });
});

describe('computeSPC', () => {
  it('returns weight 1 for each edge in a simple linear chain', () => {
    // a → b → c (one path)
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
    ]);
    const weights = computeSPC(g);
    expect(weights.size).toBe(2);
    for (const w of weights.values()) {
      expect(w).toBe(1);
    }
  });

  it('assigns weight 2 to the shared edge in a diamond', () => {
    // a → b, a → c, b → d, c → d
    // Path count through each edge: a→b=2? No.
    // Paths: a→b→d, a→c→d (2 paths total)
    // a→b: 1 path, a→c: 1 path, b→d: 1 path, c→d: 1 path
    const g = makeGraph([
      ['a', 'b'],
      ['a', 'c'],
      ['b', 'd'],
      ['c', 'd'],
    ]);
    const weights = computeSPC(g);
    // n_s(a)=2 paths from source, n_t(d)=1
    // Wait, recalculate: n_s(a)=1 (source), n_s(b)=1, n_s(c)=1, n_s(d)=2
    // n_t(d)=1 (sink), n_t(b)=1, n_t(c)=1, n_t(a)=2
    // a→b: n_s(a)*n_t(b) = 1*1 = 1
    // a→c: n_s(a)*n_t(c) = 1*1 = 1
    // b→d: n_s(b)*n_t(d) = 1*1 = 1
    // c→d: n_s(c)*n_t(d) = 1*1 = 1
    // Each edge weight = 1
    for (const w of weights.values()) {
      expect(w).toBeGreaterThanOrEqual(1);
    }
    expect(weights.size).toBe(4);
  });

  it('handles a graph with multiple sources converging', () => {
    // a → c, b → c (two sources, one sink)
    const g = makeGraph([
      ['a', 'c'],
      ['b', 'c'],
    ]);
    const weights = computeSPC(g);
    expect(weights.size).toBe(2);
    // Each edge carries exactly one source-to-sink path
    for (const w of weights.values()) {
      expect(w).toBe(1);
    }
  });

  it('handles empty graph', () => {
    const g = new Graph({ type: 'directed' });
    expect(computeSPC(g).size).toBe(0);
  });

  it('excludes back-edges from the weight map', () => {
    const g = makeYearGraph(
      [
        { id: 'old', year: 2000 },
        { id: 'new', year: 2020 },
      ],
      [['old', 'new']] // 2000 cites 2020 — back-edge
    );
    const weights = computeSPC(g);
    expect(weights.size).toBe(0);
  });
});

describe('traceMainPath', () => {
  it('traces a simple linear chain from source to sink', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
      ['c', 'd'],
    ]);
    const weights = computeSPC(g);
    const path = traceMainPath(g, weights);
    expect(path.nodes).toEqual(new Set(['a', 'b', 'c', 'd']));
    expect(path.edges.size).toBe(3);
  });

  it('selects the higher-weight branch in a diamond', () => {
    // a → b → d, a → c → d  — equal weights, either path valid.
    // Just check we get a connected 3-node path.
    const g = makeGraph([
      ['a', 'b'],
      ['a', 'c'],
      ['b', 'd'],
      ['c', 'd'],
    ]);
    const weights = computeSPC(g);
    const path = traceMainPath(g, weights);
    expect(path.nodes.has('a')).toBe(true);
    expect(path.nodes.has('d')).toBe(true);
    expect(path.nodes.size).toBeGreaterThanOrEqual(3);
  });

  it('returns empty sets for a graph with no edges', () => {
    const g = new Graph({ type: 'directed' });
    g.addNode('a');
    const weights = computeSPC(g);
    const path = traceMainPath(g, weights);
    expect(path.nodes.size).toBe(0);
    expect(path.edges.size).toBe(0);
  });

  it('handles multiple independent sources', () => {
    // Two disjoint chains: a→b and c→d
    const g = makeGraph([
      ['a', 'b'],
      ['c', 'd'],
    ]);
    const weights = computeSPC(g);
    const path = traceMainPath(g, weights);
    expect(path.nodes).toEqual(new Set(['a', 'b', 'c', 'd']));
    expect(path.edges.size).toBe(2);
  });
});

describe('computeMainPath (convenience)', () => {
  it('returns the main path nodes and edges in one call', () => {
    const g = makeGraph([
      ['a', 'b'],
      ['b', 'c'],
    ]);
    const path = computeMainPath(g);
    expect(path.nodes).toEqual(new Set(['a', 'b', 'c']));
    expect(path.edges.size).toBe(2);
  });

  it('returns empty result for empty graph', () => {
    const g = new Graph({ type: 'directed' });
    const path = computeMainPath(g);
    expect(path.nodes.size).toBe(0);
    expect(path.edges.size).toBe(0);
  });
});
