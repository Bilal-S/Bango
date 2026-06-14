import Graph from 'graphology';
import louvain from 'graphology-communities-louvain';
import forceAtlas2 from 'graphology-layout-forceatlas2';
import { clusterColor } from '../types/biblio-network';
import type { KeywordNode, KeywordEdge } from '../types/biblio-keyword';

export interface LayoutRequest {
  nodes: KeywordNode[];
  edges: KeywordEdge[];
  iterations?: number;
  layoutMode?: 'fixed' | 'dynamic';
}

export interface LayoutResponse {
  nodes: KeywordNode[];
  edges: KeywordEdge[];
  clusterCount: number;
}

self.onmessage = (event: MessageEvent<LayoutRequest>) => {
  const { nodes, edges, iterations = 100, layoutMode = 'fixed' } = event.data;

  // Reconstruct the graphology Graph object
  const g = new Graph({ type: 'undirected', multi: false });

  // Add nodes
  for (const node of nodes) {
    g.addNode(node.id, {
      label: node.label,
      weight: node.weight,
      source: node.source,
      avgYear: node.avgYear,
      rawTerms: node.rawTerms,
    });
  }

  // Add edges
  for (const edge of edges) {
    if (g.hasNode(edge.source) && g.hasNode(edge.target)) {
      // For undirected co-occurrence, avoid duplicate edges if any
      const edgeKey = `${edge.source}-${edge.target}`;
      if (!g.hasEdge(edge.source, edge.target)) {
        g.addEdgeWithKey(edgeKey, edge.source, edge.target, {
          weight: edge.weight,
        });
      }
    }
  }

  // 1. Initial circular layout
  const gNodes = g.nodes();
  const angleStep = (2 * Math.PI) / Math.max(gNodes.length, 1);
  const circularScale = 100;
  for (let i = 0; i < gNodes.length; i++) {
    const node = gNodes[i]!;
    g.setNodeAttribute(node, 'x', Math.cos(i * angleStep) * circularScale);
    g.setNodeAttribute(node, 'y', Math.sin(i * angleStep) * circularScale);
  }

  // 2. Louvain community detection
  let clusterCount = 0;
  try {
    const details = louvain.detailed(g, { resolution: 1.0 });
    const coms = details.communities;
    let maxCommunity = 0;
    for (const [nodeId, community] of Object.entries(coms)) {
      const color = clusterColor(community);
      g.setNodeAttribute(nodeId, 'cluster', community);
      g.setNodeAttribute(nodeId, 'color', color);
      if (community > maxCommunity) maxCommunity = community;
    }
    clusterCount = details.count > 0 ? details.count : maxCommunity + 1;
  } catch (err) {
    console.error('[layout.worker] louvain clustering failed', err);
  }

  // 3. ForceAtlas2 layout
  try {
    if (layoutMode === 'dynamic') {
      const shouldOptimize = g.order > 500;
      forceAtlas2.assign(g, {
        iterations,
        settings: {
          linLogMode: true,
          adjustSizes: true,
          gravity: 1,
          scalingRatio: 2,
          barnesHutOptimize: shouldOptimize,
        },
      });
    }
  } catch (err) {
    console.error('[layout.worker] ForceAtlas2 layout failed', err);
  }

  // Map result back to nodes array
  const updatedNodes = nodes.map((node) => {
    const x = g.getNodeAttribute(node.id, 'x');
    const y = g.getNodeAttribute(node.id, 'y');
    const cluster = g.getNodeAttribute(node.id, 'cluster') ?? null;
    const color = g.getNodeAttribute(node.id, 'color') ?? undefined;
    return {
      ...node,
      x,
      y,
      cluster,
      color,
    };
  });

  const response: LayoutResponse = {
    nodes: updatedNodes,
    edges,
    clusterCount,
  };

  (self as unknown as Worker).postMessage(response);
};
