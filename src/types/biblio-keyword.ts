/**
 * Types for the Keyword Co-Occurrence Network visualization module.
 */

export interface KeywordNode {
  id: string; // normalized term
  label: string; // most frequent raw term
  weight: number; // document occurrences
  source: string; // source type (metadata, tags, labels, ai_extracted, user_added)
  avgYear: number | null; // average year of publications containing this term
  rawTerms: string[]; // list of all raw terms that mapped to this stemmed term
  cluster: number | null; // community index assigned by Louvain algorithm
  color?: string; // display color assigned from cluster palette
  x?: number;
  y?: number;
  yearCounts?: { year: number; count: number }[];
}

export interface KeywordEdge {
  source: string;
  target: string;
  weight: number; // co-occurrence count
}

export interface KeywordNetworkResponse {
  nodes: KeywordNode[];
  edges: KeywordEdge[];
  meta: {
    nodeCount: number;
    edgeCount: number;
    totalArticles: number;
    sourcesActive: string[];
  };
}
