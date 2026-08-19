import { describe, it, expect } from 'vitest';
import Graph from 'graphology';
import {
  applyGraphFilters,
  applyCitationGraphFilters,
  applyCocitationGraphFilters,
  applyKeywordGraphFilters,
  applyRejectedMatchesFilter,
} from '@/utils/graph-filters';

// ─── graph builders ──────────────────────────────────────────────

/**
 * Build an undirected co-author-style graph from a compact spec.
 * Nodes carry `weight` (paper count), `label`, and optionally `maxAuthorCount`
 * on edges (the mega-author-paper guard).
 */
function makeCoAuthorGraph(
  nodes: Array<{ id: string; label: string; weight: number }>,
  edges: Array<{ source: string; target: string; weight?: number; maxAuthorCount?: number }> = []
): Graph {
  const g = new Graph({ type: 'undirected', multi: false });
  for (const n of nodes) {
    g.addNode(n.id, { label: n.label, weight: n.weight, hidden: false });
  }
  for (const e of edges) {
    g.addUndirectedEdge(e.source, e.target, {
      weight: e.weight ?? 1,
      maxAuthorCount: e.maxAuthorCount ?? 0,
      hidden: false,
    });
  }
  return g;
}

/**
 * Build a directed citation-style graph from a compact spec.
 * Nodes carry `numCited`, `label`, `title`, `authors`, `year`, and degree is
 * derived from the edge set.
 */
function makeCitationGraph(
  nodes: Array<{
    id: string;
    label?: string;
    title?: string;
    authors?: string;
    numCited?: number;
    year?: number | null;
  }>,
  edges: Array<[string, string]> = []
): Graph {
  const g = new Graph({ type: 'directed', multi: false });
  for (const n of nodes) {
    g.addNode(n.id, {
      label: n.label ?? n.id,
      title: n.title ?? '',
      authors: n.authors ?? '',
      numCited: n.numCited ?? 0,
      year: n.year ?? null,
      hidden: false,
    });
  }
  for (const [s, t] of edges) {
    if (!g.hasEdge(s, t)) g.addDirectedEdge(s, t, { weight: 1, hidden: false });
  }
  return g;
}

/**
 * Build an undirected keyword co-occurrence graph from a compact spec.
 * Nodes carry `weight` (occurrence count); edges carry `weight` (co-occurrence).
 */
function makeKeywordGraph(
  nodes: Array<{ id: string; label: string; weight: number }>,
  edges: Array<{ source: string; target: string; weight?: number }> = []
): Graph {
  const g = new Graph({ type: 'undirected', multi: false });
  for (const n of nodes) {
    g.addNode(n.id, { label: n.label, weight: n.weight, hidden: false });
  }
  for (const e of edges) {
    g.addUndirectedEdge(e.source, e.target, { weight: e.weight ?? 1, hidden: false });
  }
  return g;
}

// ─── applyGraphFilters (co-author / generic) ─────────────────────

describe('applyGraphFilters', () => {
  it('minPapers hides nodes below the weight threshold', () => {
    const g = makeCoAuthorGraph(
      [
        { id: 'a', label: 'Alice', weight: 5 },
        { id: 'b', label: 'Bob', weight: 2 },
        { id: 'c', label: 'Carol', weight: 10 },
      ],
      []
    );
    const result = applyGraphFilters(g, {
      minPapers: 3,
      minLinkStrength: 0,
      maxAuthors: Infinity,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(g.getNodeAttribute('a', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
    expect(g.getNodeAttribute('c', 'hidden')).toBe(false);
  });

  it('minPapers boundary: weight === threshold is visible (>=)', () => {
    const g = makeCoAuthorGraph(
      [
        { id: 'a', label: 'A', weight: 3 },
        { id: 'b', label: 'B', weight: 2 },
      ],
      []
    );
    applyGraphFilters(g, {
      minPapers: 3,
      minLinkStrength: 0,
      maxAuthors: Infinity,
      search: '',
    });
    expect(g.getNodeAttribute('a', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
  });

  it('search filters by case-insensitive label substring', () => {
    const g = makeCoAuthorGraph(
      [
        { id: 'a', label: 'Alice Smith', weight: 1 },
        { id: 'b', label: 'Bob Jones', weight: 1 },
        { id: 'c', label: 'Carol ALICE', weight: 1 },
      ],
      []
    );
    const result = applyGraphFilters(g, {
      minPapers: 0,
      minLinkStrength: 0,
      maxAuthors: Infinity,
      search: 'alice',
    });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
  });

  it('minLinkStrength hides edges below the weight threshold', () => {
    const g = new Graph({ type: 'undirected', multi: false });
    g.addNode('a', { label: 'A', weight: 1, hidden: false });
    g.addNode('b', { label: 'B', weight: 1, hidden: false });
    g.addUndirectedEdge('a', 'b', { weight: 2, maxAuthorCount: 0, hidden: false });
    const result = applyGraphFilters(g, {
      minPapers: 0,
      minLinkStrength: 5,
      maxAuthors: Infinity,
      search: '',
    });
    expect(result.visibleEdges).toBe(0); // edge weight 2 < 5
    expect(g.getEdgeAttribute(g.edges()[0], 'hidden')).toBe(true);
  });

  it('maxAuthors hides edges from mega-author papers', () => {
    const g = new Graph({ type: 'undirected', multi: false });
    g.addNode('a', { label: 'A', weight: 1, hidden: false });
    g.addNode('b', { label: 'B', weight: 1, hidden: false });
    g.addNode('c', { label: 'C', weight: 1, hidden: false });
    g.addUndirectedEdge('a', 'b', { weight: 1, maxAuthorCount: 50, hidden: false }); // mega
    g.addUndirectedEdge('a', 'c', { weight: 1, maxAuthorCount: 3, hidden: false }); // normal
    const result = applyGraphFilters(g, {
      minPapers: 0,
      minLinkStrength: 0,
      maxAuthors: 10,
      search: '',
    });
    expect(result.visibleEdges).toBe(1); // only a-c
  });

  it('combined filters: node hidden by weight also hides its edges', () => {
    const g = makeCoAuthorGraph(
      [
        { id: 'a', label: 'A', weight: 5 },
        { id: 'b', label: 'B', weight: 1 },
      ],
      [{ source: 'a', target: 'b', weight: 1 }]
    );
    const result = applyGraphFilters(g, {
      minPapers: 3,
      minLinkStrength: 0,
      maxAuthors: Infinity,
      search: '',
    });
    expect(result.visibleNodes).toBe(1); // a only
    expect(result.visibleEdges).toBe(0); // edge needs both endpoints visible
  });

  it('empty search shows all nodes passing the weight filter', () => {
    const g = makeCoAuthorGraph([
      { id: 'a', label: 'Alice', weight: 2 },
      { id: 'b', label: 'Bob', weight: 3 },
    ]);
    const result = applyGraphFilters(g, {
      minPapers: 0,
      minLinkStrength: 0,
      maxAuthors: Infinity,
      search: '',
    });
    expect(result.visibleNodes).toBe(2);
  });

  it('handles an empty graph gracefully', () => {
    const g = new Graph({ type: 'undirected' });
    const result = applyGraphFilters(g, {
      minPapers: 5,
      minLinkStrength: 5,
      maxAuthors: 10,
      search: 'x',
    });
    expect(result.visibleNodes).toBe(0);
    expect(result.visibleEdges).toBe(0);
  });
});

// ─── applyCitationGraphFilters ───────────────────────────────────

describe('applyCitationGraphFilters', () => {
  it('minCitations hides nodes below numCited threshold', () => {
    const g = makeCitationGraph([
      { id: 'a', numCited: 5 },
      { id: 'b', numCited: 1 },
      { id: 'c', numCited: 10 },
    ]);
    const result = applyCitationGraphFilters(g, {
      minCitations: 5,
      showIsolated: true,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
  });

  it('showIsolated=false hides nodes with zero degree', () => {
    const g = makeCitationGraph(
      [
        { id: 'a', numCited: 0 },
        { id: 'b', numCited: 0 },
        { id: 'isolated', numCited: 0 },
      ],
      [['a', 'b']]
    );
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: false,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // a + b (connected)
    expect(g.getNodeAttribute('isolated', 'hidden')).toBe(true);
  });

  it('showIsolated=true keeps zero-degree nodes', () => {
    const g = makeCitationGraph(
      [
        { id: 'a', numCited: 0 },
        { id: 'isolated', numCited: 0 },
      ],
      []
    );
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: true,
      search: '',
    });
    expect(result.visibleNodes).toBe(2);
  });

  it('search matches label, title, or authors (case-insensitive)', () => {
    const g = makeCitationGraph([
      { id: 'a', label: 'Sugar Tax', title: 'other', authors: 'nobody' },
      { id: 'b', label: 'other', title: 'OBESITY research', authors: 'nobody' },
      { id: 'c', label: 'other', title: 'other', authors: 'Dr. Smith' },
    ]);
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: true,
      search: 'smi', // matches authors on c
    });
    expect(result.visibleNodes).toBe(1);
    expect(g.getNodeAttribute('c', 'hidden')).toBe(false);
  });

  it('search matches title case-insensitively', () => {
    const g = makeCitationGraph([
      { id: 'a', label: 'X', title: 'sugar', authors: '' },
      { id: 'b', label: 'Y', title: 'salt', authors: '' },
    ]);
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: true,
      search: 'SUGAR',
    });
    expect(result.visibleNodes).toBe(1);
    expect(g.getNodeAttribute('a', 'hidden')).toBe(false);
  });

  it('yearRange hides nodes outside the range', () => {
    const g = makeCitationGraph([
      { id: 'old', year: 1990 },
      { id: 'mid', year: 2010 },
      { id: 'new', year: 2025 },
    ]);
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: true,
      search: '',
      yearRange: [2000, 2020],
    });
    expect(result.visibleNodes).toBe(1); // mid
    expect(g.getNodeAttribute('mid', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('old', 'hidden')).toBe(true);
    expect(g.getNodeAttribute('new', 'hidden')).toBe(true);
  });

  it('yearRange keeps nodes with null year (cannot evaluate)', () => {
    const g = makeCitationGraph([
      { id: 'a', year: 2010 },
      { id: 'b', year: null },
    ]);
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: true,
      search: '',
      yearRange: [2000, 2020],
    });
    expect(result.visibleNodes).toBe(2); // both pass
  });

  it('combined filters: edge needs both endpoints visible', () => {
    const g = makeCitationGraph(
      [
        { id: 'a', numCited: 10 },
        { id: 'b', numCited: 0 }, // fails minCitations
        { id: 'c', numCited: 10 },
      ],
      [
        ['a', 'b'],
        ['a', 'c'],
      ]
    );
    const result = applyCitationGraphFilters(g, {
      minCitations: 5,
      showIsolated: true,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(result.visibleEdges).toBe(1); // only a-c; a-b fails because b hidden
  });

  it('handles an empty graph gracefully', () => {
    const g = new Graph({ type: 'directed' });
    const result = applyCitationGraphFilters(g, {
      minCitations: 5,
      showIsolated: false,
      search: 'x',
      yearRange: [2000, 2020],
    });
    expect(result.visibleNodes).toBe(0);
    expect(result.visibleEdges).toBe(0);
  });

  it('yearRange=null does not filter by year', () => {
    const g = makeCitationGraph([
      { id: 'a', year: 1900 },
      { id: 'b', year: 2025 },
    ]);
    const result = applyCitationGraphFilters(g, {
      minCitations: 0,
      showIsolated: true,
      search: '',
      yearRange: null,
    });
    expect(result.visibleNodes).toBe(2);
  });
});

/**
 * Build an undirected co-citation-style graph from a compact spec.
 * Co-citation nodes carry label/title/authors/doi + coCitationCount (NOT a
 * `weight` attribute, unlike keyword graphs), matching the real
 * `use-cocitation-network.ts` buildGraph output.
 */
function makeCocitationGraph(
  nodes: Array<{
    id: string;
    label?: string;
    title?: string;
    authors?: string;
    doi?: string;
  }>,
  edges: Array<[string, string]> = []
): Graph {
  const g = new Graph({ type: 'undirected', multi: false });
  for (const n of nodes) {
    g.addNode(n.id, {
      label: n.label ?? n.id,
      title: n.title ?? '',
      authors: n.authors ?? '',
      doi: n.doi ?? '',
      coCitationCount: 1,
      hidden: false,
    });
  }
  for (const [s, t] of edges) {
    if (!g.hasEdge(s, t)) g.addUndirectedEdge(s, t, { weight: 1, hidden: false });
  }
  return g;
}

// ─── applyCocitationGraphFilters ─────────────────────────────────

describe('applyCocitationGraphFilters', () => {
  it('empty search shows all nodes', () => {
    const g = makeCocitationGraph([
      { id: 'a', label: 'Sugar Tax', title: 'other' },
      { id: 'b', label: 'other', title: 'Obesity' },
    ]);
    const result = applyCocitationGraphFilters(g, { search: '' });
    expect(result.visibleNodes).toBe(2);
    expect(g.getNodeAttribute('a', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('b', 'hidden')).toBe(false);
  });

  it('search matches label, title, authors, or doi (case-insensitive)', () => {
    const g = makeCocitationGraph([
      { id: 'a', label: 'Sugar Tax', title: 'other', authors: 'nobody', doi: '10.1/x' },
      { id: 'b', label: 'other', title: 'OBESITY research', authors: 'nobody', doi: '10.2/y' },
      { id: 'c', label: 'other', title: 'other', authors: 'Dr. Smith', doi: '10.3/z' },
      { id: 'd', label: 'other', title: 'other', authors: 'nobody', doi: '10.4/MATCH' },
    ]);
    const result = applyCocitationGraphFilters(g, { search: 'match' });
    expect(result.visibleNodes).toBe(1);
    expect(g.getNodeAttribute('d', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('a', 'hidden')).toBe(true);
  });

  it('search matches authors case-insensitively', () => {
    const g = makeCocitationGraph([
      { id: 'a', label: 'x', title: 'x', authors: 'Jane DOE' },
      { id: 'b', label: 'y', title: 'y', authors: 'smith' },
    ]);
    const result = applyCocitationGraphFilters(g, { search: 'doe' });
    expect(result.visibleNodes).toBe(1);
    expect(g.getNodeAttribute('a', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
  });

  it('edge requires both endpoints to pass the search filter', () => {
    const g = makeCocitationGraph(
      [
        { id: 'a', label: 'sugar', title: 'sugar' },
        { id: 'b', label: 'salt', title: 'salt' },
        { id: 'c', label: 'sugar-free', title: 'sugar-free' },
      ],
      [
        ['a', 'b'],
        ['a', 'c'],
      ]
    );
    const result = applyCocitationGraphFilters(g, { search: 'sugar' });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(result.visibleEdges).toBe(1); // only a-c; a-b fails because b hidden
  });

  it('clearing the search (empty string) restores all nodes', () => {
    const g = makeCocitationGraph(
      [
        { id: 'a', label: 'alpha' },
        { id: 'b', label: 'beta' },
        { id: 'c', label: 'gamma' },
      ],
      [['a', 'b']]
    );
    // First hide everything except alpha
    applyCocitationGraphFilters(g, { search: 'alpha' });
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
    // Then clear
    const result = applyCocitationGraphFilters(g, { search: '' });
    expect(result.visibleNodes).toBe(3);
    expect(result.visibleEdges).toBe(1);
    expect(g.getNodeAttribute('b', 'hidden')).toBe(false);
  });

  it('handles an empty graph gracefully', () => {
    const g = new Graph({ type: 'undirected' });
    const result = applyCocitationGraphFilters(g, { search: 'x' });
    expect(result.visibleNodes).toBe(0);
    expect(result.visibleEdges).toBe(0);
  });
});

// ─── applyKeywordGraphFilters ────────────────────────────────────

describe('applyKeywordGraphFilters', () => {
  it('minOccurrences hides nodes below the weight threshold', () => {
    const g = makeKeywordGraph([
      { id: 'sugar', label: 'sugar', weight: 10 },
      { id: 'tax', label: 'tax', weight: 2 },
      { id: 'obesity', label: 'obesity', weight: 5 },
    ]);
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 5,
      minCooccurrence: 0,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // sugar + obesity
    expect(g.getNodeAttribute('tax', 'hidden')).toBe(true);
  });

  it('minCooccurrence hides edges below the weight threshold', () => {
    const g = makeKeywordGraph(
      [
        { id: 'a', label: 'a', weight: 1 },
        { id: 'b', label: 'b', weight: 1 },
      ],
      [{ source: 'a', target: 'b', weight: 3 }]
    );
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 0,
      minCooccurrence: 5,
      search: '',
    });
    expect(result.visibleEdges).toBe(0);
    expect(g.getEdgeAttribute(g.edges()[0], 'hidden')).toBe(true);
  });

  it('search filters by case-insensitive label substring', () => {
    const g = makeKeywordGraph([
      { id: 'a', label: 'machine-learning', weight: 1 },
      { id: 'b', label: 'statistics', weight: 1 },
      { id: 'c', label: 'deep-LEARNING', weight: 1 },
    ]);
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 0,
      minCooccurrence: 0,
      search: 'learning',
    });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(g.getNodeAttribute('b', 'hidden')).toBe(true);
  });

  it('edge requires both endpoints to pass the node filter', () => {
    const g = makeKeywordGraph(
      [
        { id: 'a', label: 'a', weight: 10 },
        { id: 'b', label: 'b', weight: 1 }, // fails minOccurrences
        { id: 'c', label: 'c', weight: 10 },
      ],
      [
        { source: 'a', target: 'b', weight: 1 },
        { source: 'a', target: 'c', weight: 1 },
      ]
    );
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 5,
      minCooccurrence: 0,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // a + c
    expect(result.visibleEdges).toBe(1); // only a-c
  });

  it('combined minOccurrences + minCooccurrence filters together', () => {
    const g = makeKeywordGraph(
      [
        { id: 'a', label: 'a', weight: 10 },
        { id: 'b', label: 'b', weight: 10 },
      ],
      [
        { source: 'a', target: 'b', weight: 1 }, // fails minCooccurrence
      ]
    );
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 5,
      minCooccurrence: 3,
      search: '',
    });
    expect(result.visibleNodes).toBe(2); // both pass node filter
    expect(result.visibleEdges).toBe(0); // edge fails weight filter
  });

  it('handles an empty graph gracefully', () => {
    const g = new Graph({ type: 'undirected' });
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 5,
      minCooccurrence: 5,
      search: 'x',
    });
    expect(result.visibleNodes).toBe(0);
    expect(result.visibleEdges).toBe(0);
  });

  it('empty search shows all nodes passing the weight filter', () => {
    const g = makeKeywordGraph([
      { id: 'a', label: 'alpha', weight: 1 },
      { id: 'b', label: 'beta', weight: 2 },
    ]);
    const result = applyKeywordGraphFilters(g, {
      minOccurrences: 0,
      minCooccurrence: 0,
      search: '',
    });
    expect(result.visibleNodes).toBe(2);
  });
});

describe('applyRejectedMatchesFilter', () => {
  function makeGraph(): Graph {
    const g = new Graph({ type: 'undirected' });
    g.addNode('kept', { label: 'Kept Paper', matchedArticleStatus: 'included' });
    g.addNode('rejected', { label: 'Rejected Paper', matchedArticleStatus: 'rejected' });
    g.addNode('unmatched', { label: 'Unmatched Paper', matchedArticleStatus: null });
    g.addUndirectedEdge('kept', 'rejected');
    return g;
  }

  it('hides rejected matches when hide is true and returns the composed count', () => {
    const g = makeGraph();
    const visible = applyRejectedMatchesFilter(g, true);
    expect(g.getNodeAttribute('rejected', 'hidden')).toBe(true);
    /* Untouched nodes stay `undefined` (visible) - the helper only ever adds hiding. */
    expect(g.getNodeAttribute('kept', 'hidden')).not.toBe(true);
    expect(g.getNodeAttribute('unmatched', 'hidden')).not.toBe(true);
    expect(visible).toBe(2);
  });

  it('never hides anything when hide is false', () => {
    const g = makeGraph();
    const visible = applyRejectedMatchesFilter(g, false);
    expect(g.getNodeAttribute('rejected', 'hidden')).not.toBe(true);
    expect(visible).toBe(3);
  });

  it('does not un-hide nodes the search filter hid (regression)', () => {
    /* Regression: the old inline view version un-hid every non-rejected
     * node, silently cancelling the live search filter. The compose contract:
     * a node hidden by the search stays hidden; rejected adds hiding. */
    const g = makeGraph();
    g.setNodeAttribute('kept', 'hidden', true); // hidden by search
    g.setNodeAttribute('unmatched', 'hidden', true); // hidden by search

    const visible = applyRejectedMatchesFilter(g, true);

    expect(g.getNodeAttribute('kept', 'hidden')).toBe(true);
    expect(g.getNodeAttribute('unmatched', 'hidden')).toBe(true);
    expect(g.getNodeAttribute('rejected', 'hidden')).toBe(true);
    expect(visible).toBe(0);
  });

  it('composes with the cocitation search filter end to end', () => {
    const g = makeGraph();
    applyCocitationGraphFilters(g, { search: 'kept' });
    const visible = applyRejectedMatchesFilter(g, true);
    // Only 'kept' matches the search; the rejected match is additionally hidden.
    expect(visible).toBe(1);
    expect(g.getNodeAttribute('kept', 'hidden')).toBe(false);
    expect(g.getNodeAttribute('rejected', 'hidden')).toBe(true);
    expect(g.getNodeAttribute('unmatched', 'hidden')).toBe(true);
  });
});
