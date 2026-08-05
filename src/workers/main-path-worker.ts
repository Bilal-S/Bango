/* Computes the main path (SPC) off the main thread. Graph serialized to
 * plain arrays for structured-clone transfer; worker reconstructs graphology,
 * runs analysis, posts back node/edge ID sets. */

import Graph from 'graphology';
import { computeMainPath } from '../utils/citation-analysis';

// Serializable graph payload sent to the worker.
export interface MainPathRequest {
  nodes: { id: string; year?: number | null }[];
  edges: { id: string; source: string; target: string }[];
}

// Result payload posted back from the worker.
export interface MainPathResponse {
  nodes: string[];
  edges: string[];
}

self.onmessage = (event: MessageEvent<MainPathRequest>) => {
  const { nodes, edges } = event.data;

  const g = new Graph({ type: 'directed', multi: false });
  for (const { id, year } of nodes) {
    g.addNode(id);
    if (year !== undefined) g.setNodeAttribute(id, 'year', year);
  }
  for (const { id, source, target } of edges) {
    if (g.hasNode(source) && g.hasNode(target) && !g.hasEdge(id)) {
      g.addDirectedEdgeWithKey(id, source, target);
    }
  }

  const result = computeMainPath(g);

  const response: MainPathResponse = {
    nodes: [...result.nodes],
    edges: [...result.edges],
  };

  (self as unknown as Worker).postMessage(response);
};
