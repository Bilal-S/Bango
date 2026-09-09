import type { Ref } from 'vue';
import {
  parseToolbarSearch,
  type ToolbarSearch,
  type ToolbarFieldSearch,
} from '@/utils/toolbar-search';
import type { ArticleStatus } from '@/types';

export type TitleMatchType = 'starts_with' | 'contains' | 'ends_with' | 'exact';

export interface ArticleFilter {
  titleMatch: TitleMatchType;
  titleText: string;
  authorText: string;
  yearFrom: number | null;
  yearTo: number | null;
  journal: string;
  /** Case-insensitive partial-match DOI. Mutually exclusive with `doiEmpty`. */
  doiText: string;
  /** Restrict to articles with no DOI. */
  doiEmpty: boolean;
  tags: string[];
  labels: string[];
  /** Tags the article must NOT have. Toggled from inclusion to exclusion in the filter panel. */
  excludedTags: string[];
  /** Labels the article must NOT have. */
  excludedLabels: string[];
  /** Matched criterion UUIDs the article must have (inclusion OR exclusion arrays). */
  criteria: string[];
  /** Restrict to articles referencing deleted (unknown) criterion UUIDs. */
  criteriaUnknown: boolean;
  /** Restrict to articles with no matched criteria at all. */
  criteriaEmpty: boolean;
  /** Restrict to articles with no matched exclusion criteria (PRISMA "generally excluded"). */
  exclusionCriteriaEmpty: boolean;
}

export interface ArticleQuery {
  status: string | null;
  search: string | null;
  sortBy: string | null;
  sortDir: string | null;
  yearFrom: number | null;
  yearTo: number | null;
  manualOverrideOnly: boolean;
  screeningErrorsOnly: boolean;
  author: string | null;
  journal: string | null;
  /** Partial-match DOI. Null/empty filters nothing. */
  doi: string | null;
  /** Restrict to articles with no DOI. */
  doiEmpty: boolean;
  tags: string[];
  labels: string[];
  excludedTags: string[];
  excludedLabels: string[];
  /** Matched criterion UUIDs the article must have (AND, across both arrays). */
  matchedCriteria: string[];
  /** Restrict to articles referencing deleted (unknown) criterion UUIDs. */
  criteriaUnknown: boolean;
  /** Restrict to articles with no matched criteria at all. */
  criteriaEmpty: boolean;
  /** Restrict to articles with an empty matched-exclusion array. */
  exclusionCriteriaEmpty: boolean;
  limit: number;
  offset: number;
}

export type SortDirection = 'asc' | 'desc';

export const STATUS_TABS: readonly (ArticleStatus | 'all' | 'error' | 'references' | 'search')[] = [
  'all',
  'duplicate',
  'working',
  'included',
  'rejected',
  'error',
  'references',
  'search',
] as const;

export type StatusTab = (typeof STATUS_TABS)[number];

/**
 * Map a status tab to the backend query fields it implies. Shared by
 * `setStatusTab` (view tab clicks) and the `status` route deep-link param so
 * the `"error"` special case (working + screeningErrorsOnly) cannot drift.
 */
export function statusQueryForTab(
  tab: StatusTab
): Pick<ArticleQuery, 'status' | 'screeningErrorsOnly'> {
  if (tab === 'error') return { status: 'working', screeningErrorsOnly: true };
  return { status: tab === 'all' ? null : tab, screeningErrorsOnly: false };
}

export function createDefaultFilter(): ArticleFilter {
  return {
    titleMatch: 'contains',
    titleText: '',
    authorText: '',
    yearFrom: null,
    yearTo: null,
    journal: '',
    doiText: '',
    doiEmpty: false,
    tags: [],
    labels: [],
    excludedTags: [],
    excludedLabels: [],
    criteria: [],
    criteriaUnknown: false,
    criteriaEmpty: false,
    exclusionCriteriaEmpty: false,
  };
}

export function createDefaultQuery(defaultTab: StatusTab): ArticleQuery {
  return {
    status: defaultTab === 'all' ? null : defaultTab,
    search: null,
    sortBy: null,
    sortDir: null,
    yearFrom: null,
    yearTo: null,
    manualOverrideOnly: false,
    screeningErrorsOnly: false,
    author: null,
    journal: null,
    doi: null,
    doiEmpty: false,
    tags: [],
    labels: [],
    excludedTags: [],
    excludedLabels: [],
    matchedCriteria: [],
    criteriaUnknown: false,
    criteriaEmpty: false,
    exclusionCriteriaEmpty: false,
    limit: 10,
    offset: 0,
  };
}

/**
 * Clear all filter + query fields to defaults. Shared by the D5 route-param
 * reset (runs BEFORE param-application so incoming params overwrite the
 * cleared defaults) and `clearFilters` (which additionally resets
 * `titleMatch` and re-runs the search). Does NOT touch `titleMatch`.
 */
export function resetFilterFields(
  filter: ArticleFilter,
  query: ArticleQuery,
  searchText: Ref<string>,
  resetPage: () => void
): void {
  filter.titleText = '';
  filter.authorText = '';
  filter.yearFrom = null;
  filter.yearTo = null;
  filter.journal = '';
  filter.doiText = '';
  filter.doiEmpty = false;
  filter.tags = [];
  filter.labels = [];
  filter.excludedTags = [];
  filter.excludedLabels = [];
  filter.criteria = [];
  filter.criteriaUnknown = false;
  filter.criteriaEmpty = false;
  filter.exclusionCriteriaEmpty = false;
  query.search = null;
  query.yearFrom = null;
  query.yearTo = null;
  query.author = null;
  query.journal = null;
  query.doi = null;
  query.doiEmpty = false;
  query.tags = [];
  query.labels = [];
  query.excludedTags = [];
  query.excludedLabels = [];
  query.matchedCriteria = [];
  query.criteriaUnknown = false;
  query.criteriaEmpty = false;
  query.exclusionCriteriaEmpty = false;
  searchText.value = '';
  resetPage();
}

/**
 * True when the query carries any user-applied filter dimension (drives the
 * `isFiltered` display state: filtered pagination math, toolbar badge, etc.).
 */
export function isQueryFiltered(query: ArticleQuery): boolean {
  return !!(
    query.search ||
    query.author ||
    query.journal ||
    query.doi ||
    query.doiEmpty ||
    query.tags.length > 0 ||
    query.labels.length > 0 ||
    query.excludedTags.length > 0 ||
    query.excludedLabels.length > 0 ||
    query.matchedCriteria.length > 0 ||
    query.criteriaUnknown ||
    query.criteriaEmpty ||
    query.exclusionCriteriaEmpty ||
    query.yearFrom !== null ||
    query.yearTo !== null
  );
}

export interface ArticleFiltersDeps {
  filter: ArticleFilter;
  query: ArticleQuery;
  searchText: Ref<string>;
  activeStatusTab: Ref<StatusTab>;
  showFilters: Ref<boolean>;
  sortColumn: Ref<string | null>;
  sortDirection: Ref<SortDirection>;
  currentPage: Ref<number>;
  pageSize: Ref<number>;
  /** Resets to page 1 + `offset = 0`. */
  resetPage: () => void;
  /** Re-runs the backend query. */
  search: () => Promise<void>;
  /** Auto-opens the detail panel when a filter application yields one result. */
  autoSelectSingleResult: () => Promise<void>;
}

/**
 * Query-state mutations driven by the status tabs, column-sort headers,
 * filter panel, and toolbar search box. Extracted from `useArticleSearch`
 * (refactor1 T4.1); the parent re-exposes every function unchanged.
 */
export function useArticleFilters(deps: ArticleFiltersDeps) {
  const {
    filter,
    query,
    searchText,
    activeStatusTab,
    showFilters,
    sortColumn,
    sortDirection,
    currentPage,
    pageSize,
    resetPage,
    search,
    autoSelectSingleResult,
  } = deps;

  function setStatusTab(tab: StatusTab): void {
    activeStatusTab.value = tab;
    // "references" + "search" tabs: no article query needed - the components
    // handle their own data (ReferencesView / OpenAlexSearch).
    if (tab === 'references' || tab === 'search') {
      return;
    }
    Object.assign(query, statusQueryForTab(tab));
    resetPage();
    void search();
  }

  function toggleSort(column: string): void {
    if (sortColumn.value === column) {
      sortDirection.value = sortDirection.value === 'asc' ? 'desc' : 'asc';
    } else {
      sortColumn.value = column;
      sortDirection.value = 'asc';
    }
    query.sortBy = sortColumn.value;
    query.sortDir = sortDirection.value;
    // Keep the current page - re-sort in-place so the user sees the
    // same offset with the new sort order applied.
    query.offset = (currentPage.value - 1) * pageSize.value;
    void search();
  }

  function toggleFilters(): void {
    showFilters.value = !showFilters.value;
  }

  function applyFilters(): Promise<void> {
    query.search = filter.titleText || null;
    query.yearFrom = filter.yearFrom;
    query.yearTo = filter.yearTo;
    query.author = filter.authorText || null;
    query.journal = filter.journal || null;
    // DOI filter: when `doiEmpty` is checked the text is ignored (the backend
    // `doi_empty` branch wins over `doi` to avoid contradictory SQL).
    query.doiEmpty = filter.doiEmpty;
    query.doi = filter.doiEmpty ? null : filter.doiText.trim() || null;
    query.tags = [...filter.tags];
    query.labels = [...filter.labels];
    query.excludedTags = [...filter.excludedTags];
    query.excludedLabels = [...filter.excludedLabels];
    query.matchedCriteria = [...filter.criteria];
    query.criteriaUnknown = filter.criteriaUnknown;
    query.criteriaEmpty = filter.criteriaEmpty;
    query.exclusionCriteriaEmpty = filter.exclusionCriteriaEmpty;
    resetPage();
    return search().then(autoSelectSingleResult);
  }

  function clearFilters(): void {
    filter.titleMatch = 'contains';
    resetFilterFields(filter, query, searchText, resetPage);
    void search();
  }

  /**
   * Execute a quick search from the toolbar search box. A recognized field
   * prefix (`a:`, `d:`/`doi:`, `j:`, `y:`/`year:` - see `parseToolbarSearch`)
   * routes the text to that filter instead of the free-text search and mirrors
   * it into the panel (same both-sides sync as the `?author=` deep-link), so
   * `a:fer` is equivalent to typing `fer` in the Author field.
   */
  function executeToolbarSearch(): void {
    applyToolbarParsed(parseToolbarSearch(searchText.value));
    resetPage();
    void search();
  }

  /** Apply a parsed toolbar search: field kinds replace their panel field +
   *  query value; plain text lands in `query.search` alone (panel fields stay
   *  as last applied, matching the panel's edits-need-Apply contract). */
  function applyToolbarParsed(parsed: ToolbarSearch): void {
    switch (parsed.kind) {
      case 'author':
        filter.authorText = parsed.author;
        query.author = parsed.author;
        query.search = null;
        break;
      case 'doi':
        filter.doiText = parsed.doi;
        filter.doiEmpty = false;
        query.doi = parsed.doi;
        query.doiEmpty = false;
        query.search = null;
        break;
      case 'journal':
        filter.journal = parsed.journal;
        query.journal = parsed.journal;
        query.search = null;
        break;
      case 'year':
        filter.yearFrom = parsed.yearFrom;
        filter.yearTo = parsed.yearTo;
        query.yearFrom = parsed.yearFrom;
        query.yearTo = parsed.yearTo;
        query.search = null;
        break;
      case 'plain':
        query.search = parsed.search || null;
        break;
    }
  }

  /** True when the panel field still carries what the last toolbar field
   *  search set (user has not edited it since), so the "x" may revert it. */
  function toolbarFieldUnedited(parsed: ToolbarFieldSearch): boolean {
    switch (parsed.kind) {
      case 'author':
        return filter.authorText === parsed.author;
      case 'doi':
        return filter.doiText === parsed.doi;
      case 'journal':
        return filter.journal === parsed.journal;
      case 'year':
        return filter.yearFrom === parsed.yearFrom && filter.yearTo === parsed.yearTo;
    }
  }

  /** Reset the panel field + query value a toolbar field search had set. */
  function revertToolbarField(parsed: ToolbarFieldSearch): void {
    switch (parsed.kind) {
      case 'author':
        filter.authorText = '';
        query.author = null;
        break;
      case 'doi':
        filter.doiText = '';
        query.doi = null;
        break;
      case 'journal':
        filter.journal = '';
        query.journal = null;
        break;
      case 'year':
        filter.yearFrom = null;
        filter.yearTo = null;
        query.yearFrom = null;
        query.yearTo = null;
        break;
    }
  }

  /**
   * Clear the toolbar search and refresh results. When the last search was a
   * field search (`a:fer`), the "x" undoes the whole search: the panel field
   * it set is reverted too - unless the user edited that field afterwards.
   */
  function clearSearch(): void {
    const parsed = parseToolbarSearch(searchText.value);
    if (parsed.kind !== 'plain' && toolbarFieldUnedited(parsed)) {
      revertToolbarField(parsed);
    }
    searchText.value = '';
    query.search = null;
    resetPage();
    void search();
  }

  return {
    setStatusTab,
    toggleSort,
    toggleFilters,
    applyFilters,
    clearFilters,
    executeToolbarSearch,
    clearSearch,
  };
}
