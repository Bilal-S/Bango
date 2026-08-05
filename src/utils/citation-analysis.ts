/* Pure graph-analysis utilities for citation networks.
 * Edge convention: source cites target. Out-edges = references (ancestry),
 * in-edges = citing papers (progeny). */

import type Graph from 'graphology';

/** Compute ancestry of a node (transitive out-edges BFS). Excludes `nodeId`. */
export function computeAncestry(graph: Graph, nodeId: string): Set<string> {
  if (!graph.hasNode(nodeId)) return new Set();

  // Seed visited with the start node so cycles back to it are broken.
  const visited = new Set<string>([nodeId]);
  const queue: string[] = [];

  for (const n of graph.outNeighbors(nodeId)) {
    if (!visited.has(n)) {
      visited.add(n);
      queue.push(n);
    }
  }

  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const neighbor of graph.outNeighbors(current)) {
      if (!visited.has(neighbor)) {
        visited.add(neighbor);
        queue.push(neighbor);
      }
    }
  }

  visited.delete(nodeId); // exclude start node from result
  return visited;
}

/** Compute progeny of a node (transitive in-edges BFS). Excludes `nodeId`. */
export function computeProgeny(graph: Graph, nodeId: string): Set<string> {
  if (!graph.hasNode(nodeId)) return new Set();

  // Seed visited with the start node so cycles back to it are broken.
  const visited = new Set<string>([nodeId]);
  const queue: string[] = [];

  for (const n of graph.inNeighbors(nodeId)) {
    if (!visited.has(n)) {
      visited.add(n);
      queue.push(n);
    }
  }

  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const neighbor of graph.inNeighbors(current)) {
      if (!visited.has(neighbor)) {
        visited.add(neighbor);
        queue.push(neighbor);
      }
    }
  }

  visited.delete(nodeId); // exclude start node from result
  return visited;
}

/** Return nodes whose `year` is within [min, max] (inclusive), or null/undefined
 *  (always included). Pass `null` range to return all nodes. */
export function filterNodesByYearRange(graph: Graph, range: [number, number] | null): Set<string> {
  if (!range) return new Set(graph.nodes());

  const [min, max] = range;
  const result = new Set<string>();

  graph.forEachNode((node) => {
    const year = graph.getNodeAttribute(node, 'year') as number | null | undefined;
    if (year === null || year === undefined) {
      result.add(node);
    } else if (year >= min && year <= max) {
      result.add(node);
    }
  });

  return result;
}

/** Derive edge set connecting all pairs within `nodeSet`. For highlight/styling. */
export function computeSubgraphEdges(graph: Graph, nodeSet: Set<string>): Set<string> {
  const edges = new Set<string>();
  for (const node of nodeSet) {
    if (!graph.hasNode(node)) continue;
    graph.forEachOutEdge(node, (edge, _attrs, _source, target) => {
      if (nodeSet.has(target)) edges.add(edge);
    });
  }
  return edges;
}

// ---------------------------------------------------------------------------
// Main Path Analysis (SPC: Search Path Count)
// ---------------------------------------------------------------------------

/** Identify back-edges: year(target) > year(source), temporally impossible.
 *  Null-year endpoints are never flagged. Returns edges to exclude from DAG algorithms. */
export function findBackEdges(graph: Graph): Set<string> {
  const back = new Set<string>();
  graph.forEachEdge((edge, _attrs, source, target) => {
    const sy = graph.getNodeAttribute(source, 'year') as number | null | undefined;
    const ty = graph.getNodeAttribute(target, 'year') as number | null | undefined;
    if (sy != null && ty != null && ty > sy) {
      back.add(edge);
    }
  });
  return back;
}

/** Topological sort via Kahn's algorithm. Nodes in cycles (after excluding `skipEdges`)
 *  are omitted. Returns sources-first order. */
export function topologicalSort(graph: Graph, skipEdges?: Set<string>): string[] {
  const inDegree = new Map<string, number>();

  for (const node of graph.nodes()) {
    let deg = 0;
    graph.forEachInEdge(node, (edge) => {
      if (!skipEdges || !skipEdges.has(edge)) deg++;
    });
    inDegree.set(node, deg);
  }

  const queue: string[] = [];
  for (const [node, deg] of inDegree) {
    if (deg === 0) queue.push(node);
  }

  const result: string[] = [];
  while (queue.length > 0) {
    const node = queue.shift()!;
    result.push(node);
    graph.forEachOutEdge(node, (edge, _attrs, _source, target) => {
      if (skipEdges && skipEdges.has(edge)) return;
      const d = (inDegree.get(target) ?? 0) - 1;
      inDegree.set(target, d);
      if (d === 0) queue.push(target);
    });
  }

  return result;
}

/** Compute Search Path Count (SPC) weights for every edge.
 * Algorithm: remove back-edges → topo sort → forward pass (n_s) → backward
 * pass (n_t) → w(u→v) = n_s(u) * n_t(v). Back-edges/cycle edges are absent
 * (weight 0). */
export function computeSPC(graph: Graph): Map<string, number> {
  const backEdges = findBackEdges(graph);
  const topoOrder = topologicalSort(graph, backEdges);

  // --- Forward pass: n_s(v) = number of source-to-v paths ---
  const ns = new Map<string, number>();
  for (const node of graph.nodes()) ns.set(node, 0);

  for (const node of topoOrder) {
    let validInCount = 0;
    let sum = 0;
    graph.forEachInEdge(node, (edge, _attrs, source) => {
      if (backEdges.has(edge)) return;
      validInCount++;
      sum += ns.get(source) ?? 0;
    });
    // Sources (no valid incoming edges) start at 1
    ns.set(node, validInCount === 0 ? 1 : sum);
  }

  // --- Backward pass: n_t(u) = number of u-to-sink paths ---
  const nt = new Map<string, number>();
  for (const node of graph.nodes()) nt.set(node, 0);

  const reverseTopo = [...topoOrder].reverse();
  for (const node of reverseTopo) {
    let validOutCount = 0;
    let sum = 0;
    graph.forEachOutEdge(node, (edge, _attrs, _source, target) => {
      if (backEdges.has(edge)) return;
      validOutCount++;
      sum += nt.get(target) ?? 0;
    });
    // Sinks (no valid outgoing edges) start at 1
    nt.set(node, validOutCount === 0 ? 1 : sum);
  }

  // --- Edge weights: w(u→v) = n_s(u) * n_t(v) ---
  const weights = new Map<string, number>();
  graph.forEachEdge((edge, _attrs, source, target) => {
    if (backEdges.has(edge)) return;
    const w = (ns.get(source) ?? 0) * (nt.get(target) ?? 0);
    weights.set(edge, w);
  });

  return weights;
}

/** Trace main path: greedily follow highest-SPC-weight outgoing edge from each
 *  source to sink. Multiple sources traced independently, results unioned. */
export function traceMainPath(
  graph: Graph,
  weights: Map<string, number>
): { nodes: Set<string>; edges: Set<string> } {
  const resultNodes = new Set<string>();
  const resultEdges = new Set<string>();

  // Find source nodes: nodes with no incoming *weighted* edges, but at least
  // one outgoing weighted edge.
  const sources: string[] = [];
  for (const node of graph.nodes()) {
    let hasWeightedIn = false;
    graph.forEachInEdge(node, (edge) => {
      if (weights.has(edge)) hasWeightedIn = true;
    });
    if (hasWeightedIn) continue;

    let hasWeightedOut = false;
    graph.forEachOutEdge(node, (edge) => {
      if (weights.has(edge)) hasWeightedOut = true;
    });
    if (hasWeightedOut) sources.push(node);
  }

  // Greedy trace from each source
  for (const source of sources) {
    let current = source;
    const visited = new Set<string>([current]);

    while (true) {
      resultNodes.add(current);

      // Find the outgoing edge with the maximum SPC weight
      let bestEdge: string | null = null;
      let bestWeight = 0;

      graph.forEachOutEdge(current, (edge) => {
        const w = weights.get(edge) ?? 0;
        if (w > bestWeight) {
          bestWeight = w;
          bestEdge = edge;
        }
      });

      if (bestEdge === null) break; // sink reached

      const target = graph.target(bestEdge);
      if (visited.has(target)) break; // cycle protection

      resultEdges.add(bestEdge);
      resultNodes.add(target);
      visited.add(target);
      current = target;
    }
  }

  return { nodes: resultNodes, edges: resultEdges };
}

/** Convenience: compute main path in a single call (computeSPC + traceMainPath). */
export function computeMainPath(graph: Graph): { nodes: Set<string>; edges: Set<string> } {
  const weights = computeSPC(graph);
  return traceMainPath(graph, weights);
}
