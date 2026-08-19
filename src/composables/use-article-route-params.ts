import type { Ref } from 'vue';
import {
  STATUS_TABS,
  resetFilterFields,
  statusQueryForTab,
  type ArticleFilter,
  type ArticleQuery,
  type StatusTab,
} from './use-article-filters';

/** Accepted route params for {@link useArticleRouteParams}. */
interface RouteParams {
  status?: string;
  tags?: string[];
  labels?: string[];
  yearFrom?: number;
  yearTo?: number;
  journal?: string;
  author?: string;
  /** Keep the filter panel collapsed even with filters applied (deep-links). */
  filterCollapsed?: boolean;
  /**
   * When true, clear all `filter.*` + `query.*` fields to defaults before
   * applying incoming params. Used by bibliometric deep-links.
   */
  resetFilters?: boolean;
}

/** Minimal structural shape the tag/label helpers need from a Pinia store. */
interface IdNameResolver {
  id: string;
  name: string;
}

/**
 * Apply the `status` route param: resolve the active tab and set
 * `query.status` + `screeningErrorsOnly`. The `"error"` tab is
 * special-cased to `status='working'` + `screeningErrorsOnly=true`.
 * No-op when `status` is not a recognized {@link StatusTab} value.
 */
function applyStatusParam(
  status: string,
  activeStatusTab: Ref<StatusTab>,
  query: ArticleQuery
): void {
  if (!STATUS_TABS.includes(status as StatusTab)) return;
  activeStatusTab.value = status as StatusTab;
  Object.assign(query, statusQueryForTab(status as StatusTab));
}

/**
 * Apply `tags` + `labels` route params. Each ID is resolved to its display
 * name via the corresponding store. No-op when arrays are empty.
 */
function applyTagLabelParams(
  params: Pick<RouteParams, 'tags' | 'labels'>,
  filter: ArticleFilter,
  query: ArticleQuery,
  tagsStore: { tags: IdNameResolver[] },
  labelsStore: { labels: IdNameResolver[] },
  showPanel: boolean,
  showFilters: Ref<boolean>
): void {
  if (params.tags && params.tags.length > 0) {
    const tagNames = params.tags
      .map((id) => tagsStore.tags.find((t) => t.id === id)?.name)
      .filter((n): n is string => !!n);
    filter.tags = tagNames;
    query.tags = tagNames;
    if (showPanel) showFilters.value = true;
  }
  if (params.labels && params.labels.length > 0) {
    const labelNames = params.labels
      .map((id) => labelsStore.labels.find((l) => l.id === id)?.name)
      .filter((n): n is string => !!n);
    filter.labels = labelNames;
    query.labels = labelNames;
    if (showPanel) showFilters.value = true;
  }
}

/**
 * Apply `yearFrom`, `yearTo`, `journal`, and `author` route params.
 * Each is synced to both the display `filter` and the search `query`.
 */
function applyNumericAndTextParams(
  params: Pick<RouteParams, 'yearFrom' | 'yearTo' | 'journal' | 'author'>,
  filter: ArticleFilter,
  query: ArticleQuery,
  showPanel: boolean,
  showFilters: Ref<boolean>
): void {
  if (params.yearFrom !== undefined && Number.isFinite(params.yearFrom)) {
    filter.yearFrom = params.yearFrom;
    query.yearFrom = params.yearFrom;
    if (showPanel) showFilters.value = true;
  }
  if (params.yearTo !== undefined && Number.isFinite(params.yearTo)) {
    filter.yearTo = params.yearTo;
    query.yearTo = params.yearTo;
    if (showPanel) showFilters.value = true;
  }
  if (params.journal) {
    filter.journal = params.journal;
    query.journal = params.journal;
    if (showPanel) showFilters.value = true;
  }
  if (params.author) {
    filter.authorText = params.author;
    query.author = params.author;
    if (showPanel) showFilters.value = true;
  }
}

/**
 * Gate for the sole-result auto-select: only fire when the user explicitly
 * filtered by tag/label/year/journal/author. NOT for a bare status-only
 * deep-link.
 */
function routeHasFilterDimensions(params: RouteParams): boolean {
  return Boolean(
    (params.tags && params.tags.length > 0) ||
    (params.labels && params.labels.length > 0) ||
    (params.yearFrom !== undefined && Number.isFinite(params.yearFrom)) ||
    (params.yearTo !== undefined && Number.isFinite(params.yearTo)) ||
    params.journal ||
    params.author
  );
}

export interface ArticleRouteParamsDeps {
  filter: ArticleFilter;
  query: ArticleQuery;
  searchText: Ref<string>;
  activeStatusTab: Ref<StatusTab>;
  showFilters: Ref<boolean>;
  tagsStore: { tags: IdNameResolver[] };
  labelsStore: { labels: IdNameResolver[] };
  /** Resets to page 1 + `offset = 0` (used by the D5 reset). */
  resetPage: () => void;
  search: () => Promise<void>;
  autoSelectSingleResult: () => Promise<void>;
  /** Hard-closes the detail panel + clears the return-target back-stack. */
  resetDetailView: () => void;
}

/**
 * Route deep-link application: maps incoming `RouteParams` onto the display
 * `filter` + search `query`, then re-runs the search. Extracted from
 * `useArticleSearch` (refactor1 T4.1); the parent re-exposes
 * `applyRouteParams` unchanged.
 */
export function useArticleRouteParams(deps: ArticleRouteParamsDeps) {
  const {
    filter,
    query,
    searchText,
    activeStatusTab,
    showFilters,
    tagsStore,
    labelsStore,
    resetPage,
    search,
    autoSelectSingleResult,
    resetDetailView,
  } = deps;

  /**
   * Apply route query params as article-list filters, then search.
   *
   * When `resetFilters` is true, ALL filter fields are cleared before
   * applying incoming params (keeps keep-alive-cached ArticleList from
   * overlaying stale filters on a bibliometric deep-link).
   */
  async function applyRouteParams(params: RouteParams): Promise<void> {
    /* Compute filter-panel visibility once so every helper honors
    `filterCollapsed`. When true, the panel stays collapsed even with applied
    filters. */
    const showPanel = !params.filterCollapsed;

    // D5: optional reset-before-apply. Runs BEFORE the param-application
    // helpers so the incoming params overwrite the freshly-cleared defaults.
    if (params.resetFilters) {
      resetFilterFields(filter, query, searchText, resetPage);
      // Hard-close any open detail panel: the displayed article is from a
      // prior session and almost certainly does not match the fresh deep-link
      // filter. Hard-close (not `closeDetail()`, which would walk the
      // back-stack and re-open the previous article) and clear the back-stack
      // too so the reset is a clean slate. `autoSelectSingleResult` below can
      // still open the FRESH sole-result article.
      resetDetailView();
    }

    if (params.status) {
      applyStatusParam(params.status, activeStatusTab, query);
    }
    applyTagLabelParams(params, filter, query, tagsStore, labelsStore, showPanel, showFilters);
    applyNumericAndTextParams(params, filter, query, showPanel, showFilters);
    await search();
    /* Auto-open the detail panel when a deep-link with a filter dimension
    (tag/label/year/journal/author) yields exactly one result. Status-only
    deep-links just load the list. */
    if (routeHasFilterDimensions(params)) {
      await autoSelectSingleResult();
    }
  }

  return { applyRouteParams };
}
