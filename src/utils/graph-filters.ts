/* Pure graph-filter functions for bibliometric network views.
 * Each takes a `graphology` graph + filter bag, mutates `hidden` in place,
 * and returns visibility counts. Vue/DOM-free → unit-testable with graphology
 * (see `src/__tests__/utils/graph-filters.test.ts`).
 * Extracted from `use-sigma-renderer.ts` for testability without DOM/WebGL scaffolding. */

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

  const edgeVisible = new Map<string, boolean>();
  for (const edge of g.edges()) {
    const mac = (g.getEdgeAttribute(edge, 'maxAuthorCount') as number) ?? 0;
    edgeVisible.set(edge, mac <= maxAuthors);
  }

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

  // Edges pass if strength + maxAuthors + both endpoints are visible
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

  // Edge visible only when both endpoints are visible
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
  const { search } = filters;
  const searchLower = search.toLowerCase();

  // Search filter on label/title/authors/doi
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

  // Edge visible only when both endpoints are visible
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
