import { ref } from 'vue';
import type Graph from 'graphology';
import louvain from 'graphology-communities-louvain';
import forceAtlas2 from 'graphology-layout-forceatlas2';
import { clusterColor } from '../types/biblio-network';

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
 * Standalone community detection — does NOT update any reactive ref.
 * Use this when you only need the cluster assignments on the graph.
 */
export function detectCommunities(g: Graph): number {
  const details = louvain.detailed(g, { resolution: 1.0 });
  const coms = details.communities;
  let maxCommunity = 0;

  for (const [nodeId, community] of Object.entries(coms)) {
    const color = clusterColor(community);
    g.setNodeAttribute(nodeId, 'cluster', community);
    g.setNodeAttribute(nodeId, 'color', color);
    if (community > maxCommunity) maxCommunity = community;
  }

  return details.count > 0 ? details.count : maxCommunity + 1;
}

/**
 * Apply ForceAtlas2 layout asynchronously in chunks to keep the UI responsive.
 */
async function runForceAtlas2Async(
  g: Graph,
  iterations: number,
  layoutMode: 'fixed' | 'dynamic' = 'fixed'
): Promise<void> {
  const shouldOptimize = g.order > 500;
  const chunkSize = 25;
  let remaining = iterations;

  while (remaining > 0) {
    const chunk = Math.min(remaining, chunkSize);
    if (layoutMode === 'fixed') {
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
    } else {
      forceAtlas2.assign(g, {
        iterations: chunk,
        settings: {
          linLogMode: true,
          adjustSizes: true,
          gravity: 1,
          scalingRatio: 2,
          barnesHutOptimize: shouldOptimize,
        },
      });
    }
    remaining -= chunk;
    // Yield to the browser between chunks
    await new Promise<void>((r) => setTimeout(r, 0));
  }
}

/**
 * Composable for network layout and community detection.
 *
 * IMPORTANT: `isLayouting` and `clusterCount` are declared INSIDE the function
 * so each component instance gets its own reactive state.  A previous version
 * had these at module scope, creating a shared singleton across all callers —
 * meaning one component's layout run could clobber another's flags during route
 * transitions.
 */
export function useNetworkLayout() {
  const isLayouting = ref(false);
  const clusterCount = ref(0);

  /**
   * Detect communities using the Louvain algorithm and assign cluster colors.
   * Updates this composable instance's reactive `clusterCount` ref and returns
   * the count.  Delegates the graph mutation to the standalone export.
   */
  function detectCommunitiesReactive(g: Graph): number {
    clusterCount.value = detectCommunities(g);
    return clusterCount.value;
  }

  /**
   * Run the full layout pipeline: circular → Louvain → ForceAtlas2.
   */
  async function applyLayout(
    g: Graph,
    iterations = 100,
    layoutMode: 'fixed' | 'dynamic' = 'fixed'
  ): Promise<void> {
    isLayouting.value = true;
    try {
      // 1. Initial circular layout
      applyCircularLayout(g);

      // 2. Detect communities (assigns cluster + color, updates ref)
      detectCommunitiesReactive(g);

      // 3. Run ForceAtlas2 for final positioning
      await runForceAtlas2Async(g, iterations, layoutMode);
    } finally {
      isLayouting.value = false;
    }
  }

  return {
    isLayouting,
    clusterCount,
    applyLayout,
    applyCircularLayout,
    // Exposed under the original name for backward compatibility with callers
    // that destructure `detectCommunities` from the composable's return value.
    detectCommunities: detectCommunitiesReactive,
    runForceAtlas2Async,
  };
}
