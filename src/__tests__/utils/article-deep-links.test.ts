import { describe, it, expect } from 'vitest';
import { parseArticleRouteQuery } from '@/utils/article-deep-links';
import type { LocationQuery } from 'vue-router';

/* Pure-parser tests for the Articles view deep-link contract (refactor1 T4.2).
 * Extracted from `article-list.vue::readRouteDeepLinkParams`; the view test
 * (src/__tests__/views/article-list.test.ts) pins the apply-on-mount side. */

describe('parseArticleRouteQuery', () => {
  it('empty_query_yields_no_params_and_no_flags', () => {
    const p = parseArticleRouteQuery({});
    expect(p.status).toBeUndefined();
    expect(p.tagsParam).toBeUndefined();
    expect(p.labelsParam).toBeUndefined();
    expect(p.articleId).toBeUndefined();
    expect(p.filterCollapsed).toBe(false);
    expect(p.resetFilters).toBe(false);
    expect(p.hasFilterParams).toBe(false);
  });

  it('filter_params_are_parsed_and_flagged', () => {
    const p = parseArticleRouteQuery({
      status: 'included',
      tags: 'ml,dl',
      labels: 'priority',
      yearFrom: '2020',
      yearTo: '2022',
      journal: 'Nature',
      author: 'Smith',
    } as LocationQuery);
    expect(p.status).toBe('included');
    expect(p.tagsParam).toEqual(['ml', 'dl']);
    expect(p.labelsParam).toEqual(['priority']);
    expect(p.yearFrom).toBe(2020);
    expect(p.yearTo).toBe(2022);
    expect(p.journal).toBe('Nature');
    expect(p.author).toBe('Smith');
    expect(p.hasFilterParams).toBe(true);
  });

  it('article_id_only_does_not_set_filter_flag', () => {
    const p = parseArticleRouteQuery({ articleId: 'a-42' } as LocationQuery);
    expect(p.articleId).toBe('a-42');
    expect(p.hasFilterParams).toBe(false);
  });

  it('numeric_flags_are_strictly_one', () => {
    const p = parseArticleRouteQuery({
      filterCollapsed: '1',
      resetFilters: '1',
    } as LocationQuery);
    expect(p.filterCollapsed).toBe(true);
    expect(p.resetFilters).toBe(true);
    const off = parseArticleRouteQuery({
      filterCollapsed: '0',
      resetFilters: 'true',
    } as LocationQuery);
    expect(off.filterCollapsed).toBe(false);
    expect(off.resetFilters).toBe(false);
  });

  it('non_finite_years_do_not_set_filter_flag', () => {
    /* `Number('abc')` is NaN: the raw param is kept (parity with the view's
     * original behavior) but Number.isFinite gating keeps it out of
     * hasFilterParams. */
    const p = parseArticleRouteQuery({ yearFrom: 'abc' } as LocationQuery);
    expect(p.yearFrom).toBeNaN();
    expect(p.hasFilterParams).toBe(false);
  });

  it('non_string_values_are_ignored', () => {
    const p = parseArticleRouteQuery({
      status: ['a', 'b'],
      tags: ['x', 'y'],
      articleId: null,
    } as unknown as LocationQuery);
    expect(p.status).toBeUndefined();
    expect(p.tagsParam).toBeUndefined();
    expect(p.articleId).toBeUndefined();
    expect(p.hasFilterParams).toBe(false);
  });

  it('empty_comma_list_parses_to_single_empty_string_entry', () => {
    /* `'?tags=''.split(',')` -> [''] - preserved verbatim from the view. */
    const p = parseArticleRouteQuery({ tags: '' } as LocationQuery);
    expect(p.tagsParam).toEqual(['']);
    expect(p.hasFilterParams).toBe(true);
  });
});
