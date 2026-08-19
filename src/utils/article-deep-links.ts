/* Pure route-query parser for the Articles view deep-links (refactor1 T4.2).
 * Extracted verbatim from `article-list.vue::readRouteDeepLinkParams` so it is
 * unit-testable without a component. The view and its onActivated re-entry
 * both call `parseArticleRouteQuery(route.query)`.
 */

import type { LocationQuery } from 'vue-router';

/** Parsed deep-link params shared by `onMounted` and `onActivated` in the view. */
export interface ArticleRouteDeepLinkParams {
  status?: string;
  tagsParam?: string[];
  labelsParam?: string[];
  yearFrom?: number;
  yearTo?: number;
  journal?: string;
  author?: string;
  filterCollapsed: boolean;
  /**
   * When true, clear preserved filter/query state in cached ArticleList before
   * applying deep-link params (decision D5). Set by `buildBiblioArticleQuery`.
   */
  resetFilters: boolean;
  articleId?: string;
  hasFilterParams: boolean;
}

function asString(v: unknown): string | undefined {
  return typeof v === 'string' ? v : undefined;
}

function asNumber(v: unknown): number | undefined {
  const s = asString(v);
  return s === undefined ? undefined : Number(s);
}

function asCommaList(v: unknown): string[] | undefined {
  const s = asString(v);
  return s === undefined ? undefined : s.split(',');
}

/**
 * Read route deep-link query params. Shared by `onMounted` and `onActivated`
 * so parsing logic stays in one place.
 */
export function parseArticleRouteQuery(query: LocationQuery): ArticleRouteDeepLinkParams {
  const status = asString(query.status);
  const tagsParam = asCommaList(query.tags);
  const labelsParam = asCommaList(query.labels);
  const yearFrom = asNumber(query.yearFrom);
  const yearTo = asNumber(query.yearTo);
  const journal = asString(query.journal);
  const author = asString(query.author);
  // filterCollapsed=1 -> keep filter panel collapsed (filters still applied)
  const filterCollapsed = query.filterCollapsed === '1';
  // resetFilters=1 -> clear preserved filter/query before applying (decision D5)
  const resetFilters = query.resetFilters === '1';
  // articleId deep-link (dashboard "Go to article"): opens detail panel
  const articleId = asString(query.articleId);
  const hasFilterParams = !!(
    status ||
    tagsParam ||
    labelsParam ||
    (yearFrom !== undefined && Number.isFinite(yearFrom)) ||
    (yearTo !== undefined && Number.isFinite(yearTo)) ||
    journal ||
    author
  );
  return {
    status,
    tagsParam,
    labelsParam,
    yearFrom,
    yearTo,
    journal,
    author,
    filterCollapsed,
    resetFilters,
    articleId,
    hasFilterParams,
  };
}
