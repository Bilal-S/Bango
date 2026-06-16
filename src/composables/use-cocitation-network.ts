import { ref, computed } from 'vue';
import Graph from 'graphology';
import { tauriCommand } from './use-tauri-command';
import type {
  CocitationEdge,
  CocitationMeta,
  CocitationNetworkData,
  CocitationNode,
  CocitationParams,
} from '../types/biblio-cocitation';

const graph = ref<Graph | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
/** Diagnostic counts from the backend (article/paper/edge totals). */
const meta = ref<CocitationMeta | null>(null);

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
 * Build an undirected graphology Graph instance from raw co-citation network data.
 *
 * Node size is proportional to `coCitationCount` (how many in-scope articles
 * cite this paper), so foundational papers appear larger.
 * Edges are undirected; the `weight` field carries the selected normalization.
 */
function buildGraph(data: CocitationNetworkData): Graph {
  const g = new Graph({ type: 'undirected', multi: false });

  // Determine min/max co-citation counts for node size scaling.
  const counts = data.nodes.map((n) => n.coCitationCount);
  const minCount = Math.min(...counts, 0);
  const maxCount = Math.max(...counts, 1);

  for (const node of data.nodes) {
    g.addNode(node.id, {
      label: node.label,
      title: node.title,
      authors: node.authors,
      year: node.year,
      journal: node.journal,
      doi: node.doi,
      citationCount: node.citationCount,
      coCitationCount: node.coCitationCount,
      matchedArticleId: node.matchedArticleId,
      abstract: node.abstract,
      referenceType: node.referenceType,
      // +4 floor so 0-co-citation papers are still visible.
      size: scale(node.coCitationCount, minCount, maxCount, 5, 24),
      x: Math.random() * 100,
      y: Math.random() * 100,
      color: '#6366f1', // indigo-500 (overridden by cluster in graph component)
      cluster: null as number | null,
    });
  }

  for (const edge of data.edges) {
    if (!g.hasNode(edge.source) || !g.hasNode(edge.target)) continue;
    if (g.hasEdge(edge.source, edge.target)) continue;
    g.addUndirectedEdge(edge.source, edge.target, {
      weight: edge.weight,
      rawWeight: edge.rawWeight,
      cosineWeight: edge.cosineWeight,
      jaccardWeight: edge.jaccardWeight,
      pearsonWeight: edge.pearsonWeight,
      size: 1.5,
      color: '#cbd5e1', // slate-300
    });
  }

  return g;
}

/**
 * Composable for fetching and building the co-citation network graph.
 *
 * @param params scope, normalization mode, and threshold filters.
 */
export function useCocitationNetwork() {
  async function fetchNetwork(params: CocitationParams): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const data = await tauriCommand<CocitationNetworkData>('biblio_get_cocitation_network', {
        params,
      });

      // Always capture diagnostic meta, even when there are no nodes.
      meta.value = data?.meta ?? null;

      if (!data?.nodes?.length) {
        graph.value = null;
        return;
      }

      graph.value = buildGraph(data);
    } catch (e: unknown) {
      error.value = e instanceof Error ? e.message : String(e);
      graph.value = null;
      meta.value = null;
    } finally {
      loading.value = false;
    }
  }

  function clearGraph(): void {
    graph.value = null;
    error.value = null;
    meta.value = null;
  }

  /** Get the raw attributes of a node by id (for detail panels). */
  function getNode(nodeId: string): CocitationNode | null {
    const g = graph.value;
    if (!g || !g.hasNode(nodeId)) return null;
    const attrs = g.getNodeAttributes(nodeId);
    return {
      id: nodeId,
      label: attrs.label,
      title: attrs.title,
      authors: attrs.authors,
      year: attrs.year,
      journal: attrs.journal,
      doi: attrs.doi,
      citationCount: attrs.citationCount,
      coCitationCount: attrs.coCitationCount,
      matchedArticleId: attrs.matchedArticleId,
      abstract: attrs.abstract,
      referenceType: attrs.referenceType,
    };
  }

  /** Get the co-cited partners of a node, sorted by weight descending. */
  function getCoCitedPapers(nodeId: string): Array<{ id: string; label: string; weight: number }> {
    const g = graph.value;
    if (!g || !g.hasNode(nodeId)) return [];
    const result = g.neighbors(nodeId).map((id: string) => {
      const edgeKey = g.edge(nodeId, id);
      const edgeAttrs = edgeKey ? g.getEdgeAttributes(edgeKey) : null;
      const partnerAttrs = g.getNodeAttributes(id);
      return {
        id,
        label: partnerAttrs.label ?? id,
        weight: edgeAttrs?.weight ?? 0,
      };
    });
    return result.sort((a, b) => b.weight - a.weight);
  }

  return {
    graph,
    loading,
    error,
    meta,
    nodeCount,
    edgeCount,
    fetchNetwork,
    clearGraph,
    getNode,
    getCoCitedPapers,
  };
}

/** Re-export the edge type for component use. */
export type { CocitationEdge };
