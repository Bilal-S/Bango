import { describe, it, expect } from 'vitest';
import {
  BIBLIO_RETURN_MAP,
  resolveBiblioReturn,
  resolveCollaboratorAuthor,
  buildBiblioArticleQuery,
} from '@/utils/biblio-links';

/**
 * Narrow the `RouteLocationRaw` union (which includes `string`) to the
 * object variant so the tests can access `.name` and `.query` without
 * per-accession casts. The helper always returns the object form.
 */
function asObjectRoute(result: ReturnType<typeof buildBiblioArticleQuery>): {
  name: string;
  query: Record<string, unknown>;
} {
  expect(typeof result).toBe('object');
  expect(result).not.toBeNull();
  return result as { name: string; query: Record<string, unknown> };
}

describe('biblio-links utils', () => {
  describe('resolveBiblioReturn', () => {
    it('resolves the timeline origin', () => {
      expect(resolveBiblioReturn('timeline')).toEqual({
        name: 'timeline',
        label: 'Back to Timeline',
      });
    });

    it('resolves the authors origin', () => {
      expect(resolveBiblioReturn('authors')).toEqual({
        name: 'authors',
        label: 'Back to Authors',
      });
    });

    it('resolves the coauthors origin', () => {
      expect(resolveBiblioReturn('coauthors')).toEqual({
        name: 'coauthors',
        label: 'Back to Co-Authorship',
      });
    });

    it('resolves the keywords origin (Gap 1a)', () => {
      expect(resolveBiblioReturn('keywords')).toEqual({
        name: 'keywords',
        label: 'Back to Keywords',
      });
    });

    it('returns null for an unknown origin', () => {
      expect(resolveBiblioReturn('nonsense')).toBeNull();
    });

    it('returns null for an absent/empty from value', () => {
      expect(resolveBiblioReturn(undefined)).toBeNull();
      expect(resolveBiblioReturn(null)).toBeNull();
      expect(resolveBiblioReturn('')).toBeNull();
    });

    it('covers every key in BIBLIO_RETURN_MAP (no dead entries)', () => {
      for (const key of Object.keys(BIBLIO_RETURN_MAP)) {
        expect(resolveBiblioReturn(key)).not.toBeNull();
      }
    });
  });

  describe('resolveCollaboratorAuthor', () => {
    const rankings = [
      { displayName: 'Alice Smith' },
      { displayName: 'Bob Jones' },
      { displayName: 'Carol Lee' },
    ];

    it('finds an exact-name match', () => {
      expect(resolveCollaboratorAuthor(rankings, 'Bob Jones')).toEqual({
        displayName: 'Bob Jones',
      });
    });

    it('matches case-insensitively', () => {
      expect(resolveCollaboratorAuthor(rankings, 'alice smith')).toEqual({
        displayName: 'Alice Smith',
      });
      expect(resolveCollaboratorAuthor(rankings, 'CAROL LEE')).toEqual({
        displayName: 'Carol Lee',
      });
    });

    it('returns undefined when no ranking matches', () => {
      expect(resolveCollaboratorAuthor(rankings, 'Dave Wong')).toBeUndefined();
    });

    it('returns undefined for an empty rankings list', () => {
      expect(resolveCollaboratorAuthor([], 'Alice Smith')).toBeUndefined();
    });

    it('returns the first match when duplicates exist', () => {
      const dupes = [
        { displayName: 'Alice Smith', id: '1' },
        { displayName: 'Alice Smith', id: '2' },
      ];
      expect(resolveCollaboratorAuthor(dupes, 'Alice Smith')).toEqual({
        displayName: 'Alice Smith',
        id: '1',
      });
    });
  });

  describe('buildBiblioArticleQuery', () => {
    it('wraps the filter with the standardized status/filterCollapsed/resetFilters/from envelope', () => {
      const result = asObjectRoute(
        buildBiblioArticleQuery('timeline', { yearFrom: 2020, yearTo: 2020 })
      );
      expect(result).toEqual({
        name: 'articles',
        query: {
          yearFrom: 2020,
          yearTo: 2020,
          status: 'included',
          filterCollapsed: '1',
          resetFilters: '1',
          from: 'timeline',
        },
      });
    });

    it('passes author filter through for the authors origin', () => {
      const result = asObjectRoute(buildBiblioArticleQuery('authors', { author: 'Jane Doe' }));
      expect(result.query).toMatchObject({ author: 'Jane Doe', from: 'authors' });
    });

    it('passes journal filter through for the timeline journal-bar origin', () => {
      const result = asObjectRoute(buildBiblioArticleQuery('timeline', { journal: 'The Lancet' }));
      expect(result.query).toMatchObject({ journal: 'The Lancet', from: 'timeline' });
    });

    it('passes tags filter through for the keywords origin (Gap 1a tags source)', () => {
      const result = asObjectRoute(buildBiblioArticleQuery('keywords', { tags: ['obesity'] }));
      expect(result.query).toMatchObject({ tags: ['obesity'], from: 'keywords' });
    });

    it('passes labels filter through for the keywords origin (Gap 1a labels source)', () => {
      const result = asObjectRoute(
        buildBiblioArticleQuery('keywords', { labels: ['priority-read'] })
      );
      expect(result.query).toMatchObject({ labels: ['priority-read'], from: 'keywords' });
    });

    it('always sets status=included (decision D1 enforcement)', () => {
      // Even with an empty filter the envelope must carry status=included.
      const result = asObjectRoute(buildBiblioArticleQuery('coauthors', {}));
      expect(result.query.status).toBe('included');
    });

    it('always sets filterCollapsed=1 (decision D4 enforcement)', () => {
      const result = asObjectRoute(buildBiblioArticleQuery('coauthors', {}));
      expect(result.query.filterCollapsed).toBe('1');
    });

    it('always sets resetFilters=1 (decision D5 enforcement)', () => {
      // The biblio deep-link MUST reset any preserved filter state in the
      // cached ArticleList before applying its own filter, so a fresh biblio
      // filter does not overlay stale filters from a prior session. Even with
      // an empty filter the envelope must carry resetFilters=1.
      const result = asObjectRoute(buildBiblioArticleQuery('coauthors', {}));
      expect(result.query.resetFilters).toBe('1');
    });

    it('lets the envelope override a caller-supplied status so D1 cannot be bypassed', () => {
      // The envelope keys (status/filterCollapsed/resetFilters/from) are
      // spread AFTER the filter, so even a mistakenly-passed status is
      // overwritten. This documents the precedence so a future maintainer
      // does not think the caller can bypass D1 by passing `status: 'all'`.
      const result = asObjectRoute(
        buildBiblioArticleQuery('timeline', { yearFrom: 2020, status: 'all' })
      );
      expect(result.query.status).toBe('included');
    });

    it('merges filter and envelope without mutating the input filter object', () => {
      const filter = { author: 'Jane Doe' };
      buildBiblioArticleQuery('authors', filter);
      // The input object must not gain status/filterCollapsed/resetFilters/from keys.
      expect(filter).toEqual({ author: 'Jane Doe' });
    });

    it('targets the articles route name', () => {
      const result = asObjectRoute(buildBiblioArticleQuery('authors', { author: 'Jane Doe' }));
      expect(result.name).toBe('articles');
    });
  });
});
