/* Pure graph-filter functions for bibliometric network views.
 * Each takes a `graphology` graph + filter bag, mutates `hidden` in place,
 * and returns visibility counts. Vue/DOM-free → unit-testable with graphology
 * (see `src/__tests__/utils/graph-filters.test.ts`).
 * Extracted from `use-sigma-renderer.ts` for testability without DOM/WebGL scaffolding. */

import type Graph from 'graphology';
import { filterNodesByYearRange } from './citation-analysis';

/* Shared visibility scaffold used by every filter below: marks nodes hidden
 * via `isNodeVisible`, then edges hidden unless `isEdgeVisible` passes AND
 * both endpoints are visible. Mutates `hidden` in place; returns counts. */
function applyVisibility(
  g: Graph,
  isNodeVisible: (node: string) => boolean,
  isEdgeVisible: (edge: string) => boolean = () => true
): { visibleNodes: number; visibleEdges: number } {
  const nodeVisible = new Map<string, boolean>();
  for (const node of g.nodes()) {
    const visible = isNodeVisible(node);
    nodeVisible.set(node, visible);
    g.setNodeAttribute(node, 'hidden', !visible);
  }

  let visibleEdges = 0;
  for (const edge of g.edges()) {
    const source = g.source(edge);
    const target = g.target(edge);
    const visible =
      isEdgeVisible(edge) && nodeVisible.get(source) === true && nodeVisible.get(target) === true;
    g.setEdgeAttribute(edge, 'hidden', !visible);
    if (visible) visibleEdges++;
  }

  const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

  return { visibleNodes, visibleEdges };
}

// ─── co-author / generic network ─────────────────────────────────

/** Filter inputs for the co-author / generic network graph. */
export interface CoAuthorGraphFilters {
  minPapers: number;
  minLinkStrength: number;
  maxAuthors: number;
  search: string;
}

/* Apply visibility filters to a co-author graph. Mutates `hidden` in place.
 * - `minPapers`/`minLinkStrength`: threshold on node/edge `weight`.
 * - `maxAuthors`: drop edges exceeding maxAuthorCount (mega-author papers).
 * - `search`: case-insensitive substring match on node `label`. */
export function applyGraphFilters(
  g: Graph,
  filters: CoAuthorGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { minPapers, minLinkStrength, maxAuthors, search } = filters;
  const searchLower = search.toLowerCase();

  return applyVisibility(
    g,
    (node) => {
      const weight = g.getNodeAttribute(node, 'weight') as number;
      const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
      return weight >= minPapers && (!searchLower || label.toLowerCase().includes(searchLower));
    },
    /* Edge passes strength + maxAuthors; visibility additionally requires
     * both endpoints visible. */
    (edge) => {
      const weight = g.getEdgeAttribute(edge, 'weight') as number;
      const mac = (g.getEdgeAttribute(edge, 'maxAuthorCount') as number) ?? 0;
      return weight >= minLinkStrength && mac <= maxAuthors;
    }
  );
}

// ─── citation network ────────────────────────────────────────────

/** Filter inputs for the citation network graph. */
export interface CitationGraphFilters {
  minCitations: number;
  showIsolated: boolean;
  search: string;
  yearRange?: [number, number] | null;
}

/* Apply visibility filters to a citation graph. Mutates `hidden` in place.
 * - `minCitations`: hide papers below N incoming citations.
 * - `showIsolated`: when false, hide nodes with zero degree.
 * - `search`: case-insensitive match on `label`/`title`/`authors`.
 * - `yearRange`: when set, hide nodes outside [min, max] (null-year always visible). */
export function applyCitationGraphFilters(
  g: Graph,
  filters: CitationGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { minCitations, showIsolated, search, yearRange } = filters;
  const searchLower = search.toLowerCase();

  const yearPassSet = filterNodesByYearRange(g, yearRange ?? null);

  return applyVisibility(g, (node) => {
    const numCited = (g.getNodeAttribute(node, 'numCited') as number) ?? 0;
    const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
    const title = (g.getNodeAttribute(node, 'title') as string) ?? '';
    const authors = (g.getNodeAttribute(node, 'authors') as string) ?? '';
    const degree = g.degree(node);

    return (
      numCited >= minCitations &&
      (showIsolated || degree > 0) &&
      (!searchLower ||
        label.toLowerCase().includes(searchLower) ||
        title.toLowerCase().includes(searchLower) ||
        authors.toLowerCase().includes(searchLower)) &&
      yearPassSet.has(node)
    );
  });
}

// ─── co-citation network ─────────────────────────────────────────

/** Filter inputs for the co-citation network graph. */
export interface CocitationGraphFilters {
  search: string;
}

/* Search-only visibility filter for a co-citation graph. The co-citation
 * thresholds are backend query params (re-fetched via `onParamsChange`), not
 * client-side filters. Only the live search box is client-side.
 *
 * Does NOT reuse `applyKeywordGraphFilters` because co-citation nodes lack a
 * `weight` attribute (they carry `coCitationCount`/`citationCount`), so the
 * keyword function's `weight >= minOccurrences` would hide everything.
 *
 * Mutates `hidden` in place; returns visible counts. */
export function applyCocitationGraphFilters(
  g: Graph,
  filters: CocitationGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const searchLower = filters.search.toLowerCase();

  return applyVisibility(g, (node) => {
    const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
    const title = (g.getNodeAttribute(node, 'title') as string) ?? '';
    const authors = (g.getNodeAttribute(node, 'authors') as string) ?? '';
    const doi = (g.getNodeAttribute(node, 'doi') as string) ?? '';
    return (
      !searchLower ||
      label.toLowerCase().includes(searchLower) ||
      title.toLowerCase().includes(searchLower) ||
      authors.toLowerCase().includes(searchLower) ||
      doi.toLowerCase().includes(searchLower)
    );
  });
}

/* Compose the "hide rejected-article matches" filter on top of any existing
 * `hidden` state (e.g. the live search filter). Only ever ADDS hiding for
 * rejected matches; never un-hides, so a node hidden by the search filter
 * stays hidden (regression: the old inline version un-hid every non-rejected
 * node, silently cancelling the search filter).
 *
 * Returns the composed visible-node count. */
export function applyRejectedMatchesFilter(g: Graph, hide: boolean): number {
  let visible = 0;
  g.forEachNode((node) => {
    if (hide && g.getNodeAttribute(node, 'matchedArticleStatus') === 'rejected') {
      g.setNodeAttribute(node, 'hidden', true);
    }
    if (g.getNodeAttribute(node, 'hidden') !== true) visible++;
  });
  return visible;
}

// ─── keyword network ─────────────────────────────────────────────

/** Filter inputs for the keyword co-occurrence network graph. */
export interface KeywordGraphFilters {
  minOccurrences: number;
  minCooccurrence: number;
  search: string;
}

/* Apply visibility filters to a keyword co-occurrence graph. Mutates `hidden` in place.
 * - `minOccurrences`: hide term nodes whose `weight` is below threshold.
 * - `minCooccurrence`: hide edges whose `weight` is below threshold.
 * - `search`: case-insensitive substring match on node `label`. */
export function applyKeywordGraphFilters(
  g: Graph,
  filters: KeywordGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { minOccurrences, minCooccurrence, search } = filters;
  const searchLower = search.toLowerCase();

  return applyVisibility(
    g,
    (node) => {
      const weight = g.getNodeAttribute(node, 'weight') as number;
      const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
      return (
        weight >= minOccurrences && (!searchLower || label.toLowerCase().includes(searchLower))
      );
    },
    (edge) => (g.getEdgeAttribute(edge, 'weight') as number) >= minCooccurrence
  );
}
