/**
 * Types for the Co-Authorship Network visualization module.
 * These mirror the enriched JSON returned by `biblio_get_coauthor_network`.
 */

/** Counting mode for co-authorship link strength. */
export type CountingMode = 'full' | 'fractional';

/** A single node (author) in the co-authorship network. */
export interface CoAuthorNode {
  id: string;
  label: string;
  /** Number of articles this author appears on. */
  weight: number;
  /** Total citations across the author's articles. */
  totalCitations: number;
  /** Average publication year of the author's articles. */
  avgYear: number | null;
  /** Estimated h-index based on available data. */
  estimatedHIndex: number | null;
  /** Cluster ID assigned by community detection (Louvain). */
  cluster: number | null;
  /** Display color (assigned from cluster palette). */
  color?: string;
}

/** A normalized institution entity. */
export interface BiblioInstitution {
  id: string;
  normalizedName: string;
  country: string | null;
  city: string | null;
  createdAt: string;
}

/** A single edge (co-authorship link) in the network. */
export interface CoAuthorEdge {
  source: string;
  target: string;
  /** Number of co-authored publications (full counting). */
  weight: number;
  /** Fractional counting weight (each paper contributes total=1 split among pairs). */
  fractionalWeight: number;
  /** Max number of authors on any article contributing to this edge. */
  maxAuthorCount: number;
}

/** Raw network payload from the backend. */
export interface NetworkData {
  nodes: CoAuthorNode[];
  edges: CoAuthorEdge[];
}

/** Okabe-Ito categorical palette for cluster coloring. */
export const CLUSTER_PALETTE = [
  '#E69F00', // orange
  '#56B4E9', // sky blue
  '#009E73', // bluish green
  '#F0E442', // yellow
  '#0072B2', // blue
  '#D55E00', // vermillion
  '#CC79A7', // reddish purple
  '#999999', // gray
] as const;

/** Year → count pair, used for publications-by-year sparklines. */
export interface YearCount {
  year: number;
  count: number;
}

/** Get a color for a cluster index (wraps around palette). */
export function clusterColor(cluster: number): string {
  return CLUSTER_PALETTE[cluster % CLUSTER_PALETTE.length]!;
}
