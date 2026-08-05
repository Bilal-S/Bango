/* Co-Citation Network types. Undirected: edge A-B = frequently cited together. */

/** A single node (reference paper) in the co-citation network. */
export interface CocitationNode {
  id: string;
  /** Short label, e.g. "Smith et al. (2020)". */
  label: string;
  title: string;
  authors: string;
  year: number | null;
  journal: string | null;
  doi: string | null;
  /** Total citation count from the reference_papers table. */
  citationCount: number;
  /** How many in-scope articles cite this paper (used for node sizing). */
  coCitationCount: number;
  /** FK → articles.id when the reference paper is matched to a library article. */
  matchedArticleId: string | null;
  /**
   * Status of the matched library article (e.g. `'included'`, `'rejected'`,
   * `'working'`, `'duplicate'`), or `null` when the paper is unmatched. Used
   * by the detail panel to render an "In Library:Rejected" badge and by the
   * "Hide rejected matches" toggle.
   */
  matchedArticleStatus: string | null;
  abstract: string;
  referenceType: string | null;
}

/** A single undirected edge (co-citation link) in the network. */
export interface CocitationEdge {
  source: string;
  target: string;
  /** Selected normalization weight (depends on the requested mode). */
  weight: number;
  /** Raw co-citation count (how many articles cite both papers). */
  rawWeight: number;
  /** Cosine-normalized weight (always computed). */
  cosineWeight: number;
  /** Jaccard-normalized weight (always computed). */
  jaccardWeight: number;
  /** Pearson correlation weight (always computed when mode = pearson). */
  pearsonWeight: number;
}

/** Diagnostic counts returned alongside the network. */
export interface CocitationMeta {
  nodeCount: number;
  edgeCount: number;
  inScopeArticleCount: number;
  referencePaperCount: number;
  candidatePaperCount: number;
  scope: 'included' | 'all';
  normalization: 'raw' | 'cosine' | 'jaccard' | 'pearson';
}

/** Raw network payload from the backend. */
export interface CocitationNetworkData {
  nodes: CocitationNode[];
  edges: CocitationEdge[];
  meta: CocitationMeta;
}

/** Parameters for the co-citation network command. */
export interface CocitationParams {
  scope: 'included' | 'all';
  normalization: 'raw' | 'cosine' | 'jaccard' | 'pearson';
  minCitationCount: number;
  minCoCitation: number;
}
