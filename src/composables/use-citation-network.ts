import { ref, computed } from 'vue';
import Graph from 'graphology';
import { tauriCommand } from './use-tauri-command';
import type {
  CitationNetworkData,
  CitationNetworkMeta,
  CitationNode,
} from '../types/biblio-citation';

const graph = ref<Graph | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
/** Diagnostic counts from the backend (article/paper/edge totals). */
const meta = ref<CitationNetworkMeta | null>(null);

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
 * Build a directed graphology Graph instance from raw citation network data.
 *
 * Node size is proportional to `numCited` (total citations received), so highly
 * cited papers appear larger.  Edges are directed (`source cites target`).
 *
 * Unmatched reference leaves (papers not linked to an included article) are
 * rendered smaller and with a muted color; their connecting edges are dashed.
 */
function buildGraph(data: CitationNetworkData): Graph {
  const g = new Graph({ type: 'directed', multi: false });

  // Determine min/max citation counts for node size scaling.
  // Only consider matched (real article) nodes for the scale range so that
  // unmatched leaves (always 0 citations) don't compress the scale.
  const matchedCited = data.nodes.filter((n) => !n.unmatched).map((n) => n.numCited);
  const minCited = Math.min(...matchedCited, 0);
  const maxCited = Math.max(...matchedCited, 1);

  for (const node of data.nodes) {
    const isUnmatched = node.unmatched === true;
    g.addNode(node.id, {
      label: node.label,
      title: node.title,
      authors: node.authors,
      year: node.year,
      journal: node.journal,
      numCited: node.numCited,
      numReferences: node.numReferences,
      abstract: node.abstract,
      // Unmatched leaves are always small; real articles scale with citations.
      // +2 floor so zero-citation papers are still visible.
      size: isUnmatched ? 3 : scale(node.numCited, minCited, maxCited, 4, 22),
      x: Math.random() * 100,
      y: Math.random() * 100,
      // Unmatched leaves get a muted grey; real articles default to indigo.
      color: isUnmatched ? '#94a3b8' : '#6366f1', // slate-400 : indigo-500
      cluster: null as number | null,
      unmatched: isUnmatched,
    });
  }

  for (const edge of data.edges) {
    if (!g.hasNode(edge.source) || !g.hasNode(edge.target)) continue;
    if (g.hasEdge(edge.source, edge.target)) continue;
    const isUnmatched = edge.unmatched === true;
    g.addDirectedEdge(edge.source, edge.target, {
      weight: edge.weight,
      thickness: isUnmatched ? 0.5 : 1.0,
      // Sigma's edge/arrow programs read `size` (not `thickness`) for the
      // stroke width. Unmatched leaves get a thinner stroke so their arrows
      // stay visually subordinate to real citation edges.
      size: isUnmatched ? 0.8 : 2,
      // Unmatched edges are faint dashed lines; real citation edges are solid.
      color: isUnmatched ? '#e2e8f0' : '#cbd5e1', // slate-200 : slate-300
      type: 'arrow',
      dashed: isUnmatched,
      unmatched: isUnmatched,
    });
  }

  return g;
}

/**
 * Composable for fetching and building the citation network graph.
 *
 * @param includeUnmatched when true, request unmatched reference papers as
 *   dashed leaf nodes from the backend.  Defaults to false.
 */
export function useCitationNetwork() {
  async function fetchNetwork(includeUnmatched = false): Promise<void> {
    loading.value = true;
    error.value = null;

    try {
      const data = await tauriCommand<CitationNetworkData>(
        'biblio_get_citation_network',
        includeUnmatched ? { includeUnmatched: true } : undefined
      );

      // Always capture diagnostic meta, even when there are no nodes, so the
      // empty-state can explain why the graph is sparse.
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
  function getNode(nodeId: string): CitationNode | null {
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
      numCited: attrs.numCited,
      numReferences: attrs.numReferences,
      abstract: attrs.abstract,
      cluster: attrs.cluster,
      color: attrs.color,
      unmatched: attrs.unmatched,
    };
  }

  /** Get the ids of papers that cite the given paper (incoming edges). */
  function getCitingPapers(nodeId: string): string[] {
    const g = graph.value;
    if (!g || !g.hasNode(nodeId)) return [];
    return g.inNeighbors(nodeId);
  }

  /** Get the ids of papers cited by the given paper (outgoing edges). */
  function getCitedPapers(nodeId: string): string[] {
    const g = graph.value;
    if (!g || !g.hasNode(nodeId)) return [];
    return g.outNeighbors(nodeId);
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
    getCitingPapers,
    getCitedPapers,
  };
}
