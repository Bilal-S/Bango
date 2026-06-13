/**
 * Pure graph-analysis utilities for citation networks.
 *
 * These functions operate on a graphology directed graph instance but contain
 * no Vue reactivity, making them trivial to unit-test in isolation.
 *
 * Edge convention: `source cites target`. Therefore:
 * - Out-edges from a paper → papers it cites (its references / ancestry).
 * - In-edges to a paper → papers that cite it (its progeny / citing papers).
 */

import type Graph from 'graphology';

/**
 * Compute the **ancestry** of a node: every paper that the given paper
 * transitively *cites* (BFS over out-edges).
 *
 * The returned set does NOT include `nodeId` itself — callers that need the
 * node included should add it explicitly.
 */
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

/**
 * Compute the **progeny** of a node: every paper that transitively *cites*
 * the given paper (BFS over in-edges).
 *
 * The returned set does NOT include `nodeId` itself.
 */
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

/**
 * Return the set of node IDs whose `year` attribute falls within `range`
 * (inclusive), or whose year is null/undefined (always included — we cannot
 * evaluate them).
 *
 * @param graph     A graphology graph with optional `year` node attributes.
 * @param range     `[minYear, maxYear]` inclusive, or `null` to return all nodes.
 * @returns         Set of node IDs that pass the year filter.
 */
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

/**
 * Derive the edge set that connects all pairs of nodes within `nodeSet`.
 *
 * Useful for highlight/styling operations where we want to brighten edges
 * whose both endpoints belong to the isolated subgraph.
 */
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
// Phase 3 — Main Path Analysis (SPC: Search Path Count)
// ---------------------------------------------------------------------------

/**
 * Identify **back-edges** that violate temporal ordering.
 *
 * An edge `source → target` (source cites target) is a back-edge when
 * `year(target) > year(source)` — the cited paper is *newer* than the citing
 * paper, which is temporally impossible and indicates a data error.
 *
 * Edges where either endpoint has a null/undefined year are never flagged
 * (we cannot evaluate them).
 *
 * @returns Set of edge IDs to exclude from DAG-based algorithms.
 */
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

/**
 * Topological sort using Kahn's algorithm.
 *
 * Nodes involved in cycles (not reachable to in-degree 0 after excluding
 * `skipEdges`) are omitted from the result.  This means the returned array
 * may be shorter than `graph.nodes()` when the graph still contains cycles
 * after back-edge removal — those nodes simply don't participate in the DP.
 *
 * @param skipEdges  Edge IDs to ignore (e.g. back-edges from `findBackEdges`).
 * @returns           Node IDs in topological order (sources first).
 */
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

/**
 * Compute **Search Path Count (SPC)** traversal weights for every edge.
 *
 * SPC measures the number of source-to-sink paths that traverse each edge.
 * The algorithm:
 *
 * 1. Remove back-edges (enforce DAG by year heuristic).
 * 2. Topological sort.
 * 3. Forward pass: `n_s(v) = Σ n_s(u)` over incoming edges; sources start at 1.
 * 4. Backward pass: `n_t(u) = Σ n_t(v)` over outgoing edges; sinks start at 1.
 * 5. Edge weight: `w(u→v) = n_s(u) * n_t(v)`.
 *
 * @returns Map of edge ID → SPC weight.  Back-edges and edges in cycles
 *          are absent from the map (weight 0).
 */
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

/**
 * Trace the **main path** by greedily following the highest-SPC-weight
 * outgoing edge from each source to a sink.
 *
 * Produces *connected* paths, not scattered edge fragments.  If multiple
 * sources exist, each is traced independently and the results are unioned.
 *
 * @param weights  SPC weights from `computeSPC`.
 * @returns         `{ nodes, edges }` — the node and edge IDs on the main path.
 */
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

/**
 * Convenience: compute the main path in a single call.
 *
 * Combines `computeSPC` + `traceMainPath`.  The graph is not mutated.
 *
 * @returns `{ nodes, edges }` — the node and edge IDs on the main path,
 *          or empty sets if no path exists.
 */
export function computeMainPath(graph: Graph): { nodes: Set<string>; edges: Set<string> } {
  const weights = computeSPC(graph);
  return traceMainPath(graph, weights);
}
