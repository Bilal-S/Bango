/**
 * Pure helpers for the Bibliometrics -> Article list cross-linking feature.
 *
 * Extracted from the consuming views (`article-list.vue`,
 * `biblio-authors.vue`) so the routing/lookup logic is unit-testable without
 * mounting the full Vue components (which require Tauri, Pinia, router, and
 * chart-library mocking).
 */
import type { LocationQueryRaw, LocationQueryValueRaw, RouteLocationRaw } from 'vue-router';

/**
 * Origins that offer a "Back to ..." return button from the article list.
 *
 * Data-driven so adding a new bibliometric deep-link origin is one table row
 * instead of three new `v-if` branches. A missing key collapses `fromBiblio`
 * to `false`, so the back button simply does not render for unknown origins.
 */
export const BIBLIO_RETURN_MAP: Record<string, { name: string; label: string }> = {
  timeline: { name: 'timeline', label: 'Back to Timeline' },
  authors: { name: 'authors', label: 'Back to Authors' },
  coauthors: { name: 'coauthors', label: 'Back to Co-Authorship' },
  keywords: { name: 'keywords', label: 'Back to Keywords' },
};

/**
 * Known bibliometric deep-link origins.
 *
 * Kept in sync with the keys of {@link BIBLIO_RETURN_MAP}. `buildBiblioArticleQuery`
 * accepts this union so the `from` value is compile-time-checked at every
 * outbound call site.
 */
export type BiblioOrigin = 'timeline' | 'authors' | 'coauthors' | 'keywords';

/**
 * Resolve a `from` route-query value to its return-target descriptor.
 *
 * @param from - The `route.query.from` value (may be `undefined`/unknown).
 * @returns The `{ name, label }` descriptor, or `null` if `from` is not a
 *   known bibliometric origin.
 */
export function resolveBiblioReturn(
  from: string | undefined | null
): { name: string; label: string } | null {
  if (!from) return null;
  return BIBLIO_RETURN_MAP[from] ?? null;
}

/**
 * Build the `router.push` target for a bibliometric -> article-list deep-link.
 *
 * Centralizes the outbound payload (`status`, `filterCollapsed`, `from`,
 * `resetFilters`) so decisions D1 ("standardize on `status: 'included'`") and
 * D5 ("reset any preserved filter state before applying the deep-link") are
 * enforced in one place instead of by convention at each call site. The
 * status-agnostic `articleId`-only deep-link (Recent Papers) bypasses this
 * helper by design because the target article may be in any status.
 *
 * @param from - The bibliometric origin (becomes `route.query.from`).
 * @param filter - The origin-specific filter params (e.g. `{ author }`,
 *   `{ journal }`, `{ yearFrom, yearTo }`, `{ tags }`, `{ labels }`).
 * @returns A `RouteLocationRaw` carrying the filter plus the standardized
 *   `status`/`filterCollapsed`/`resetFilters`/`from` envelope, ready for
 *   `router.push`.
 */
export function buildBiblioArticleQuery(
  from: BiblioOrigin,
  filter: Record<string, LocationQueryValueRaw | LocationQueryValueRaw[]>
): RouteLocationRaw {
  const query: LocationQueryRaw = {
    ...filter,
    status: 'included',
    filterCollapsed: '1',
    // D5: clear any filter state the cached ArticleList kept from a previous
    // session before applying `filter`. Without this, an overlay-only
    // application would yield e.g. `author="Bob" AND yearFrom=2020` when the
    // user clicked the Co-Authorship Papers box while a year filter was still
    // active. The biblio metric summarized the included corpus with no extra
    // filters, so the list must reflect exactly the deep-link's filter.
    resetFilters: '1',
    from,
  };
  return { name: 'articles', query };
}

/**
 * Find the author ranking whose display name matches a collaborator name,
 * case-insensitively. Used by the Top Collaborators in-place selection in
 * the Author Productivity detail panel: clicking a collaborator selects the
 * matching author row without routing.
 *
 * @returns The matching rank object, or `undefined` if no ranking matches.
 */
export function resolveCollaboratorAuthor<T extends { displayName: string }>(
  rankings: readonly T[],
  collaboratorName: string
): T | undefined {
  const target = collaboratorName.toLowerCase();
  return rankings.find((r) => r.displayName.toLowerCase() === target);
}
