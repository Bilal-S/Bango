import { describe, it, expect } from 'vitest';
import Graph from 'graphology';
import { collectClusterMembers } from '@/utils/cluster-members';

function makeGraph(): Graph {
  const g = new Graph();
  g.addNode('author-1', { label: 'Alice', cluster: 0, weight: 3 });
  g.addNode('author-2', { label: 'Bob', cluster: 0, weight: 1 });
  g.addNode('author-3', { label: 'Carol', cluster: 1, weight: 2 });
  g.addNode('author-4', { label: 'Dan', cluster: null, weight: 1 });
  return g;
}

describe('collectClusterMembers', () => {
  it('collectClusterMembers_returns_matching_nodes', () => {
    const members = collectClusterMembers(makeGraph(), 0);
    expect(members).toHaveLength(2);
    expect(members).toContainEqual({ id: 'author-1', label: 'Alice' });
    expect(members).toContainEqual({ id: 'author-2', label: 'Bob' });
  });

  it('collectClusterMembers_ignores_unclustered_nodes', () => {
    // Cluster 1 contains only Carol; Dan (null cluster) and cluster-0 nodes
    // are excluded.
    const members = collectClusterMembers(makeGraph(), 1);
    expect(members).toEqual([{ id: 'author-3', label: 'Carol' }]);

    const none = collectClusterMembers(makeGraph(), 2);
    expect(none).toEqual([]);
  });

  it('collectClusterMembers_skips_hidden_nodes', () => {
    // Bob is filtered out of the visible graph (applyVisibility set
    // hidden=true): only Alice is collected for cluster 0.
    const g = makeGraph();
    g.setNodeAttribute('author-2', 'hidden', true);
    const members = collectClusterMembers(g, 0);
    expect(members).toEqual([{ id: 'author-1', label: 'Alice' }]);
  });
});
