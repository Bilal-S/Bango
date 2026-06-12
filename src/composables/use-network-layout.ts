import { ref } from 'vue';
import type Graph from 'graphology';
import louvain from 'graphology-communities-louvain';
import forceAtlas2 from 'graphology-layout-forceatlas2';
import { clusterColor } from '../types/biblio-network';

const isLayouting = ref(false);
const clusterCount = ref(0);

/**
 * Apply a circular layout to all nodes as an initial arrangement.
 * Useful before running ForceAtlas2 so nodes start in a predictable distribution.
 */
export function applyCircularLayout(g: Graph, scale = 100): void {
  const nodes = g.nodes();
  const angleStep = (2 * Math.PI) / Math.max(nodes.length, 1);
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i]!;
    g.setNodeAttribute(node, 'x', Math.cos(i * angleStep) * scale);
    g.setNodeAttribute(node, 'y', Math.sin(i * angleStep) * scale);
  }
}

/**
 * Detect communities using the Louvain algorithm and assign cluster colors.
 * Returns the number of detected communities.
 */
export function detectCommunities(g: Graph): number {
  const details = louvain.detailed(g, { resolution: 1.0 });
  // details.communities is {[nodeId: string]: communityNumber}
  const coms = details.communities;
  let maxCommunity = 0;

  for (const [nodeId, community] of Object.entries(coms)) {
    const color = clusterColor(community);
    g.setNodeAttribute(nodeId, 'cluster', community);
    g.setNodeAttribute(nodeId, 'color', color);
    if (community > maxCommunity) maxCommunity = community;
  }

  clusterCount.value = details.count > 0 ? details.count : maxCommunity + 1;
  return clusterCount.value;
}

/**
 * Apply ForceAtlas2 layout asynchronously in chunks to keep the UI responsive.
 */
async function runForceAtlas2Async(g: Graph, iterations: number): Promise<void> {
  const shouldOptimize = g.order > 500;
  const chunkSize = 25;
  let remaining = iterations;

  while (remaining > 0) {
    const chunk = Math.min(remaining, chunkSize);
    forceAtlas2(g, {
      iterations: chunk,
      settings: {
        linLogMode: true,
        adjustSizes: true,
        gravity: 1,
        scalingRatio: 2,
        barnesHutOptimize: shouldOptimize,
      },
    });
    remaining -= chunk;
    // Yield to the browser between chunks
    await new Promise<void>((r) => setTimeout(r, 0));
  }
}

/**
 * Composable for network layout and community detection.
 */
export function useNetworkLayout() {
  /**
   * Run the full layout pipeline: circular → Louvain → ForceAtlas2.
   */
  async function applyLayout(g: Graph, iterations = 100): Promise<void> {
    isLayouting.value = true;
    try {
      // 1. Initial circular layout
      applyCircularLayout(g);

      // 2. Detect communities (assigns cluster + color)
      detectCommunities(g);

      // 3. Run ForceAtlas2 for final positioning
      await runForceAtlas2Async(g, iterations);
    } finally {
      isLayouting.value = false;
    }
  }

  return {
    isLayouting,
    clusterCount,
    applyLayout,
    applyCircularLayout,
    detectCommunities,
    runForceAtlas2Async,
  };
}
