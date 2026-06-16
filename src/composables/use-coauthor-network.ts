import { ref, computed } from 'vue';
import Graph from 'graphology';
import { tauriCommand } from './use-tauri-command';
import type { NetworkData, CountingMode } from '../types/biblio-network';

const graph = ref<Graph | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const countingMode = ref<CountingMode>('full');

const nodeCount = computed(() => graph.value?.order ?? 0);
const edgeCount = computed(() => graph.value?.size ?? 0);

/**
 * Scale a numeric value from one range to another.
 */
function scale(
  value: number,
  inMin: number,
  inMax: number,
  outMin: number,
  outMax: number
): number {
  if (inMax === inMin) return (outMin + outMax) / 2;
  return outMin + ((value - inMin) / (inMax - inMin)) * (outMax - outMin);
}

/**
 * Build a graphology Graph instance from raw network data.
 */
function buildGraph(data: NetworkData): Graph {
  const g = new Graph({ type: 'undirected', multi: false });

  // Determine min/max article counts for node size scaling
  const weights = data.nodes.map((n) => n.weight);
  const minW = Math.min(...weights, 1);
  const maxW = Math.max(...weights, 1);

  // Add nodes with enriched attributes
  for (const node of data.nodes) {
    g.addNode(node.id, {
      label: node.label,
      size: scale(node.weight, minW, maxW, 3, 20),
      x: Math.random() * 100,
      y: Math.random() * 100,
      color: '#94a3b8', // default slate - will be overridden by clustering
      weight: node.weight,
      totalCitations: node.totalCitations,
      avgYear: node.avgYear,
      estimatedHIndex: node.estimatedHIndex,
      cluster: null as number | null,
    });
  }

  // Determine min/max edge weights for thickness scaling
  const edgeWeights = data.edges.map((e) => e.weight);
  const minEW = Math.min(...edgeWeights, 1);
  const maxEW = Math.max(...edgeWeights, 1);

  // Also track fractional range for mode switching
  const fracWeights = data.edges.map((e) => e.fractionalWeight ?? 0);
  const minFrac = Math.min(...fracWeights, 0.01);
  const maxFrac = Math.max(...fracWeights, 0.01);

  // Add edges - store BOTH full and fractional weights
  for (const edge of data.edges) {
    if (!g.hasNode(edge.source) || !g.hasNode(edge.target)) continue;
    // Avoid duplicate edges
    if (g.hasEdge(edge.source, edge.target)) continue;
    const fw = edge.fractionalWeight ?? edge.weight / Math.max(edge.weight, 1);
    g.addUndirectedEdge(edge.source, edge.target, {
      weight: edge.weight,
      fullWeight: edge.weight,
      fractionalWeight: fw,
      maxAuthorCount: edge.maxAuthorCount ?? 0,
      thickness: scale(edge.weight, minEW, maxEW, 0.5, 4),
      minFull: minEW,
      maxFull: maxEW,
      minFrac,
      maxFrac,
      color: '#e2e8f0',
    });
  }

  return g;
}

/**
 * Composable for fetching and building the co-authorship network graph.
 */
export function useCoAuthorNetwork() {
  /**
   * Fetch co-authorship network data from the backend and build a graphology Graph.
   */
  async function fetchNetwork(_countingMode?: CountingMode): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const data = await tauriCommand<NetworkData>('biblio_get_coauthor_network');

      if (!data?.nodes?.length) {
        graph.value = null;
        return;
      }

      graph.value = buildGraph(data);
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      graph.value = null;
    } finally {
      loading.value = false;
    }
  }

  function clearGraph(): void {
    graph.value = null;
    error.value = null;
    countingMode.value = 'full';
  }

  /**
   * Switch the active counting mode and recompute edge weights/thickness.
   * Returns true if the mode actually changed.
   */
  function setCountingMode(mode: CountingMode): boolean {
    if (mode === countingMode.value) return false;
    const g = graph.value;
    if (!g) return false;

    const isFull = mode === 'full';

    // Collect new weight range for thickness rescaling
    const minKey = isFull ? 'minFull' : 'minFrac';
    const maxKey = isFull ? 'maxFull' : 'maxFrac';

    // All edges share the same min/max stored on first edge
    const firstEdge = g.edges()[0];
    if (!firstEdge) {
      countingMode.value = mode;
      return true;
    }
    const minVal = g.getEdgeAttribute(firstEdge, minKey) as number;
    const maxVal = g.getEdgeAttribute(firstEdge, maxKey) as number;

    // Swap every edge's active weight and thickness
    for (const eid of g.edges()) {
      const w = g.getEdgeAttribute(eid, isFull ? 'fullWeight' : 'fractionalWeight') as number;
      g.setEdgeAttribute(eid, 'weight', w);
      g.setEdgeAttribute(eid, 'thickness', scale(w, minVal, maxVal, 0.5, 4));
    }

    countingMode.value = mode;
    return true;
  }

  return {
    graph,
    loading,
    error,
    nodeCount,
    edgeCount,
    countingMode,
    fetchNetwork,
    clearGraph,
    setCountingMode,
  };
}
