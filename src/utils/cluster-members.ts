import type Graph from 'graphology';

/** A cluster member entity sent to the backend command. */
export interface ClusterMember {
  id: string;
  label: string;
}

/**
 * Collect `{ id, label }` members for the graph nodes whose `cluster`
 * attribute equals `clusterIndex`.
 *
 * Only visible nodes are collected: nodes whose `hidden` attribute is true
 * (set by `utils/graph-filters.ts::applyVisibility`) are skipped, because a
 * recalculate re-clusters the visible subgraph and hidden nodes can carry
 * stale cluster ids from the previous clustering.
 *
 * Pure graph util shared by every network view (co-authorship, keyword;
 * citation/co-citation when they adopt thematic analysis). Nodes with a null
 * `cluster` or a different cluster index are excluded. The node key is the
 * member `id` (a `biblio_authors.id` UUID on the co-authorship network, a
 * normalized-term string on the keyword network).
 */
export function collectClusterMembers(graph: Graph, clusterIndex: number): ClusterMember[] {
  const members: ClusterMember[] = [];
  graph.forEachNode((node, attrs) => {
    if (attrs.cluster === clusterIndex && attrs.hidden !== true) {
      members.push({ id: node, label: attrs.label ?? node });
    }
  });
  return members;
}
