/**
 * Pure graph-filter functions for the bibliometric network views.
 *
 * Each function takes a `graphology` graph + a filter bag, mutates the graph's
 * `hidden` attributes in place, and returns `{ visibleNodes, visibleEdges }`
 * counts. They contain no Vue reactivity and no DOM/Sigma coupling, so they
 * are trivially unit-testable with a real graphology instance (see
 * `src/__tests__/utils/graph-filters.test.ts`).
 *
 * Extracted from `composables/use-sigma-renderer.ts` (Tier 1 testability) so
 * the highest-complexity untested code in the graph subsystem gets coverage
 * without the DOM + WebGL scaffolding the renderer itself requires.
 */

import type Graph from 'graphology';
import { filterNodesByYearRange } from './citation-analysis';

// ─── co-author / generic network ─────────────────────────────────

/** Filter inputs for the co-author / generic network graph. */
export interface CoAuthorGraphFilters {
  minPapers: number;
  minLinkStrength: number;
  maxAuthors: number;
  search: string;
}

/**
 * Apply visibility filters to a co-author (or generic) graph based on
 * `minPapers`, `minLinkStrength`, `maxAuthors`, and a search query.
 *
 * - `minPapers`: hide nodes whose `weight` is below the threshold.
 * - `minLinkStrength`: hide edges whose `weight` is below the threshold.
 * - `maxAuthors`: hide edges whose `maxAuthorCount` exceeds the threshold
 *   (mega-author papers). Applied first so the node/edge pass sets agree.
 * - `search`: case-insensitive substring match on the node `label`.
 *
 * Mutates `hidden` attributes on the graph and returns the visible counts.
 */
export function applyGraphFilters(
  g: Graph,
  filters: CoAuthorGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { minPapers, minLinkStrength, maxAuthors, search } = filters;
  const searchLower = search.toLowerCase();

  // First, determine which edges pass the maxAuthors filter.
  // An edge whose maxAuthorCount exceeds the threshold means it comes from
  // a mega-author paper, so we drop that edge.
  const edgeVisible = new Map<string, boolean>();
  for (const edge of g.edges()) {
    const mac = (g.getEdgeAttribute(edge, 'maxAuthorCount') as number) ?? 0;
    edgeVisible.set(edge, mac <= maxAuthors);
  }

  // Determine which nodes pass the filter
  const nodeVisible = new Map<string, boolean>();
  for (const node of g.nodes()) {
    const weight = g.getNodeAttribute(node, 'weight') as number;
    const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
    const passesPapers = weight >= minPapers;
    const passesSearch = !searchLower || label.toLowerCase().includes(searchLower);
    const visible = passesPapers && passesSearch;
    nodeVisible.set(node, visible);
    g.setNodeAttribute(node, 'hidden', !visible);
  }

  // Then, determine which edges pass all filters
  let visibleEdges = 0;
  for (const edge of g.edges()) {
    const weight = g.getEdgeAttribute(edge, 'weight') as number;
    const source = g.source(edge);
    const target = g.target(edge);
    const passesStrength = weight >= minLinkStrength;
    const passesMaxAuthors = edgeVisible.get(edge) !== false;
    const bothEndsVisible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
    const visible = passesStrength && passesMaxAuthors && bothEndsVisible;
    g.setEdgeAttribute(edge, 'hidden', !visible);
    if (visible) visibleEdges++;
  }

  const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

  return { visibleNodes, visibleEdges };
}

// ─── citation network ────────────────────────────────────────────

/** Filter inputs for the citation network graph. */
export interface CitationGraphFilters {
  minCitations: number;
  showIsolated: boolean;
  search: string;
  yearRange?: [number, number] | null;
}

/**
 * Apply visibility filters to a citation graph based on `minCitations`,
 * `showIsolated`, a search query, and an optional year range.
 *
 * - `minCitations`: hide papers with fewer than N incoming citations.
 * - `showIsolated`: when false, hide nodes with zero degree (no edges).
 * - `search`: case-insensitive substring match on `label`/`title`/`authors`.
 * - `yearRange`: when set, hide nodes whose `year` falls outside [min, max].
 *   Nodes with null/undefined year are always visible (cannot be evaluated).
 *
 * Mutates `hidden` attributes on the graph and returns the visible counts.
 */
export function applyCitationGraphFilters(
  g: Graph,
  filters: CitationGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { minCitations, showIsolated, search, yearRange } = filters;
  const searchLower = search.toLowerCase();

  // Pre-compute the set of nodes passing the year filter once (O(n)),
  // rather than recomputing inside the per-node loop.
  const yearPassSet = filterNodesByYearRange(g, yearRange ?? null);

  // Determine which nodes pass the filter
  const nodeVisible = new Map<string, boolean>();
  for (const node of g.nodes()) {
    const numCited = (g.getNodeAttribute(node, 'numCited') as number) ?? 0;
    const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
    const title = (g.getNodeAttribute(node, 'title') as string) ?? '';
    const authors = (g.getNodeAttribute(node, 'authors') as string) ?? '';
    const degree = g.degree(node);

    const passesCitations = numCited >= minCitations;
    const passesIsolated = showIsolated || degree > 0;
    const passesSearch =
      !searchLower ||
      label.toLowerCase().includes(searchLower) ||
      title.toLowerCase().includes(searchLower) ||
      authors.toLowerCase().includes(searchLower);
    const passesYear = yearPassSet.has(node);
    const visible = passesCitations && passesIsolated && passesSearch && passesYear;
    nodeVisible.set(node, visible);
    g.setNodeAttribute(node, 'hidden', !visible);
  }

  // Edges are visible only if both endpoints are visible
  let visibleEdges = 0;
  for (const edge of g.edges()) {
    const source = g.source(edge);
    const target = g.target(edge);
    const visible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
    g.setEdgeAttribute(edge, 'hidden', !visible);
    if (visible) visibleEdges++;
  }

  const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

  return { visibleNodes, visibleEdges };
}

// ─── co-citation network ─────────────────────────────────────────

/** Filter inputs for the co-citation network graph. */
export interface CocitationGraphFilters {
  search: string;
}

/**
 * Apply a search-only visibility filter to a co-citation graph.
 *
 * Unlike the citation network, the co-citation thresholds (`minCitationCount`,
 * `minCoCitation`, normalization, scope) are backend query params (re-fetched
 * via `onParamsChange`), not client-side filters. The only client-side filter
 * is the live search box, which matches `label`/`title`/`authors`/`doi`
 * case-insensitively.
 *
 * Critically, this does NOT reuse `applyKeywordGraphFilters` because
 * co-citation nodes lack a `weight` attribute (they carry `coCitationCount` /
 * `citationCount`), so the keyword function's `weight >= minOccurrences` check
 * fails for every node and hides the entire graph.
 *
 * Mutates `hidden` attributes on the graph and returns the visible counts.
 */
export function applyCocitationGraphFilters(
  g: Graph,
  filters: CocitationGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { search } = filters;
  const searchLower = search.toLowerCase();

  // Determine which nodes pass the search filter
  const nodeVisible = new Map<string, boolean>();
  for (const node of g.nodes()) {
    const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
    const title = (g.getNodeAttribute(node, 'title') as string) ?? '';
    const authors = (g.getNodeAttribute(node, 'authors') as string) ?? '';
    const doi = (g.getNodeAttribute(node, 'doi') as string) ?? '';
    const passesSearch =
      !searchLower ||
      label.toLowerCase().includes(searchLower) ||
      title.toLowerCase().includes(searchLower) ||
      authors.toLowerCase().includes(searchLower) ||
      doi.toLowerCase().includes(searchLower);
    nodeVisible.set(node, passesSearch);
    g.setNodeAttribute(node, 'hidden', !passesSearch);
  }

  // Edges are visible only if both endpoints are visible
  let visibleEdges = 0;
  for (const edge of g.edges()) {
    const source = g.source(edge);
    const target = g.target(edge);
    const visible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
    g.setEdgeAttribute(edge, 'hidden', !visible);
    if (visible) visibleEdges++;
  }

  const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

  return { visibleNodes, visibleEdges };
}

// ─── keyword network ─────────────────────────────────────────────

/** Filter inputs for the keyword co-occurrence network graph. */
export interface KeywordGraphFilters {
  minOccurrences: number;
  minCooccurrence: number;
  search: string;
}

/**
 * Apply visibility filters to a keyword co-occurrence graph based on
 * `minOccurrences`, `minCooccurrence`, and a search query.
 *
 * - `minOccurrences`: hide term nodes whose `weight` is below the threshold.
 * - `minCooccurrence`: hide edges whose `weight` is below the threshold.
 * - `search`: case-insensitive substring match on the node `label`.
 *
 * Mutates `hidden` attributes on the graph and returns the visible counts.
 */
export function applyKeywordGraphFilters(
  g: Graph,
  filters: KeywordGraphFilters
): { visibleNodes: number; visibleEdges: number } {
  const { minOccurrences, minCooccurrence, search } = filters;
  const searchLower = search.toLowerCase();

  // Determine which nodes pass the filter
  const nodeVisible = new Map<string, boolean>();
  for (const node of g.nodes()) {
    const weight = g.getNodeAttribute(node, 'weight') as number;
    const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
    const passesOccurrences = weight >= minOccurrences;
    const passesSearch = !searchLower || label.toLowerCase().includes(searchLower);
    const visible = passesOccurrences && passesSearch;
    nodeVisible.set(node, visible);
    g.setNodeAttribute(node, 'hidden', !visible);
  }

  // Determine which edges pass all filters
  let visibleEdges = 0;
  for (const edge of g.edges()) {
    const weight = g.getEdgeAttribute(edge, 'weight') as number;
    const source = g.source(edge);
    const target = g.target(edge);
    const passesStrength = weight >= minCooccurrence;
    const bothEndsVisible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
    const visible = passesStrength && bothEndsVisible;
    g.setEdgeAttribute(edge, 'hidden', !visible);
    if (visible) visibleEdges++;
  }

  const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

  return { visibleNodes, visibleEdges };
}
