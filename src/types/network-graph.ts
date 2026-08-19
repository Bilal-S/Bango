import type Graph from 'graphology';

/** Node color strategy shared by all bibliometric graph components. */
export type NetworkColorMode = 'cluster' | 'temporal';

/**
 * Shared props contract for the four bibliometric graph components
 * (`citation-network-graph`, `cocitation-network-graph`, `keyword-network-graph`,
 * `network-graph`). Domain components extend this with their own props via
 * `defineProps<NetworkGraphProps & { ... }>()`.
 */
export interface NetworkGraphProps {
  graph: Graph | null;
  loading: boolean;
  isLayouting: boolean;
  error: string | null;
  focusedNodeId: string | null;
  selectedClusters: number[];
  colorMode: NetworkColorMode;
  minYear: number;
  maxYear: number;
  /** Bumped by the parent after subgraph re-layouts so visual state re-applies. */
  recalculateTrigger: number;
}

/** One autocomplete entry rendered by `network-search-box.vue`. */
export interface NetworkSearchSuggestion {
  /** Stable `v-for` key (domain id/label). */
  key: string;
  /** Main text rendered in the dropdown row and copied into the input on select. */
  display: string;
  /** Optional trailing detail (e.g. `4 papers` for author rows). */
  detail?: string;
  /** Domain payload forwarded with `select` / `select-first` (paper label, keyword, author name). */
  payload: string;
}
