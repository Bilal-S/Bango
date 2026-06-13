/**
 * Types for the Citation Network visualization module.
 * These mirror the enriched JSON returned by `biblio_get_citation_network`.
 *
 * Unlike the co-authorship network, the citation network is a **directed**
 * graph: an edge `source → target` means *source cites target*.
 */

/** A single node (article/paper) in the citation network. */
export interface CitationNode {
  id: string;
  /** Short label, e.g. "Smith et al. (2020)". */
  label: string;
  title: string;
  authors: string;
  year: number | null;
  journal: string | null;
  /** Total citations the article has received. */
  numCited: number;
  /** Number of references the article cites. */
  numReferences: number;
  abstract: string;
  /** Cluster ID assigned by community detection. */
  cluster: number | null;
  /** Display color (assigned from cluster palette). */
  color?: string;
  /**
   * True when the node is an unmatched reference paper (not linked to an
   * included article).  Used to render dashed/faint leaf nodes.
   */
  unmatched?: boolean;
}

/** A single directed edge (citation link) in the network. */
export interface CitationEdge {
  /** Citing article. */
  source: string;
  /** Cited article. */
  target: string;
  weight: number;
  /**
   * True when this edge connects an article to an unmatched reference leaf.
   * Used to render dashed/faint edges.
   */
  unmatched?: boolean;
}

/**
 * Diagnostic counts returned alongside the network.  The frontend uses these
 * to render an informative empty-state (e.g. "0 of 120 reference papers
 * matched") so users understand why a graph may be sparse.
 */
export interface CitationNetworkMeta {
  includedArticleCount: number;
  referencePaperCount: number;
  matchedPaperCount: number;
  edgeCount: number;
}

/** Raw network payload from the backend. */
export interface CitationNetworkData {
  nodes: CitationNode[];
  edges: CitationEdge[];
  /** Optional diagnostic block (always present from backend, but optional
   * on the client for backwards compatibility). */
  meta?: CitationNetworkMeta;
}

/** Okabe-Ito categorical palette for cluster coloring. */
export const CITATION_CLUSTER_PALETTE = [
  '#E69F00', // orange
  '#56B4E9', // sky blue
  '#009E73', // bluish green
  '#F0E442', // yellow
  '#0072B2', // blue
  '#D55E00', // vermillion
  '#CC79A7', // reddish purple
  '#999999', // gray
] as const;

/** Get a color for a cluster index (wraps around palette). */
export function citationClusterColor(cluster: number): string {
  return CITATION_CLUSTER_PALETTE[cluster % CITATION_CLUSTER_PALETTE.length]!;
}
