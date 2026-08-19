import { ref, shallowRef, onUnmounted } from 'vue';
import Graph from 'graphology';
import { tauriCommand } from './use-tauri-command';
import { createBiblioNetworkState, scaleToRange } from './use-biblio-network-fetch';
import type { KeywordNetworkResponse } from '../types/biblio-keyword';
import type { LayoutRequest, LayoutResponse } from '../workers/layout.worker';

// Module-scoped state so the graph remains populated when navigating away/back
// (the shared bundle keeps the historical module-scope refs for this file).
const { graph, loading, error, nodeCount, edgeCount } = createBiblioNetworkState();
const clusterCount = ref(0);
const isLayouting = ref(false);

const sources = ref<string[]>(['metadata', 'ai_extracted', 'tags', 'labels', 'user_added']);
const minOccurrences = ref(2);
const minCooccurrence = ref(2);

function buildGraph(response: LayoutResponse): Graph {
  const g = new Graph({ type: 'undirected', multi: false });

  const weights = response.nodes.map((n) => n.weight);
  const minW = Math.min(...weights, 1);
  const maxW = Math.max(...weights, 1);

  for (const node of response.nodes) {
    g.addNode(node.id, {
      label: node.label,
      size: scaleToRange(node.weight, minW, maxW, 6, 26),
      x: node.x ?? Math.random() * 100,
      y: node.y ?? Math.random() * 100,
      color: node.color ?? '#94a3b8',
      weight: node.weight,
      source: node.source,
      avgYear: node.avgYear,
      rawTerms: node.rawTerms,
      cluster: node.cluster,
      yearCounts: node.yearCounts,
    });
  }

  // Scale edge weights
  const edgeWeights = response.edges.map((e) => e.weight);
  const minEW = Math.min(...edgeWeights, 1);
  const maxEW = Math.max(...edgeWeights, 1);

  for (const edge of response.edges) {
    if (!g.hasNode(edge.source) || !g.hasNode(edge.target)) continue;
    if (g.hasEdge(edge.source, edge.target)) continue;
    g.addUndirectedEdge(edge.source, edge.target, {
      weight: edge.weight,
      thickness: scaleToRange(edge.weight, minEW, maxEW, 0.5, 5),
      color: '#cbd5e1',
    });
  }

  return g;
}

export function useKeywordNetwork() {
  const worker = shallowRef<Worker | null>(null);
  let isUnmounted = false;

  function terminateWorker(): void {
    if (worker.value) {
      worker.value.terminate();
      worker.value = null;
    }
  }

  function getWorker(): Worker | null {
    if (worker.value) return worker.value;
    try {
      worker.value = new Worker(new URL('../workers/layout.worker.ts', import.meta.url), {
        type: 'module',
      });
      worker.value.onmessage = (event: MessageEvent<LayoutResponse>) => {
        if (isUnmounted) return;
        graph.value = buildGraph(event.data);
        clusterCount.value = event.data.clusterCount;
        isLayouting.value = false;
        loading.value = false;
      };
      worker.value.onerror = (e: ErrorEvent) => {
        console.error('[layout-worker] error', e.message);
        if (isUnmounted) return;
        isLayouting.value = false;
        loading.value = false;
        error.value = 'Failed layout computation: ' + e.message;
      };
      return worker.value;
    } catch (err) {
      console.error('[layout-worker] initialization failed', err);
      return null;
    }
  }

  async function fetchNetwork(layoutMode: 'fixed' | 'dynamic' = 'fixed'): Promise<void> {
    loading.value = true;
    error.value = null;
    isLayouting.value = false;

    try {
      const response = await tauriCommand<KeywordNetworkResponse>('biblio_get_keyword_network', {
        sources: sources.value,
        minOccurrences: minOccurrences.value,
        minCooccurrence: minCooccurrence.value,
      });

      if (!response || !response.nodes || response.nodes.length === 0) {
        graph.value = null;
        clusterCount.value = 0;
        loading.value = false;
        return;
      }

      // Start the worker to compute Louvain community detection and ForceAtlas2 layout
      const w = getWorker();
      if (w) {
        isLayouting.value = true;
        const req: LayoutRequest = {
          nodes: response.nodes,
          edges: response.edges,
          iterations: 150, // standard iteration count
          layoutMode,
        };
        w.postMessage(req);
      } else {
        console.warn('Worker fallback used.');
        graph.value = buildGraph({
          nodes: response.nodes,
          edges: response.edges,
          clusterCount: 0,
        });
        loading.value = false;
      }
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      graph.value = null;
      loading.value = false;
    }
  }

  function clearGraph(): void {
    graph.value = null;
    error.value = null;
    clusterCount.value = 0;
    isLayouting.value = false;
    terminateWorker();
  }

  onUnmounted(() => {
    isUnmounted = true;
    terminateWorker();
  });

  return {
    graph,
    loading,
    error,
    nodeCount,
    edgeCount,
    clusterCount,
    isLayouting,
    sources,
    minOccurrences,
    minCooccurrence,
    fetchNetwork,
    clearGraph,
  };
}
