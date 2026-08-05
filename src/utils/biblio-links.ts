/* Pure helpers for Bibliometrics -> Article list cross-linking.
 * Extracted from consuming views so routing/lookup logic is unit-testable
 * without full Vue component scaffolding (Tauri, Pinia, router, charts). */
import type { LocationQueryRaw, LocationQueryValueRaw, RouteLocationRaw } from 'vue-router';

/**
 * "Back to ..." return button targets from the article list. Data-driven:
 * adding a new biblio origin is one table row; a missing key collapses
 * `fromBiblio` to `false`.
 */
export const BIBLIO_RETURN_MAP: Record<string, { name: string; label: string }> = {
  timeline: { name: 'timeline', label: 'Back to Timeline' },
  authors: { name: 'authors', label: 'Back to Authors' },
  coauthors: { name: 'coauthors', label: 'Back to Co-Authorship' },
  keywords: { name: 'keywords', label: 'Back to Keywords' },
};

/** Known bibliometric deep-link origins, synced with {@link BIBLIO_RETURN_MAP}. */
export type BiblioOrigin = 'timeline' | 'authors' | 'coauthors' | 'keywords';

/** Resolve a `from` route-query value to its return-target descriptor. */
export function resolveBiblioReturn(
  from: string | undefined | null
): { name: string; label: string } | null {
  if (!from) return null;
  return BIBLIO_RETURN_MAP[from] ?? null;
}

/**
 * Build the `router.push` target for a bibliometric -> article-list deep-link.
 * Centralizes `status`/`filterCollapsed`/`resetFilters`/`from` envelope so
 * decisions D1 ("always `status: 'included'`") and D5 ("reset any preserved
 * filter state") are enforced in one place. The status-agnostic `articleId`-
 * only deep-link bypasses this helper (target may be in any status).
 */
export function buildBiblioArticleQuery(
  from: BiblioOrigin,
  filter: Record<string, LocationQueryValueRaw | LocationQueryValueRaw[]>
): RouteLocationRaw {
  const query: LocationQueryRaw = {
    ...filter,
    status: 'included',
    filterCollapsed: '1',
    resetFilters: '1',
    from,
  };
  return { name: 'articles', query };
}

/**
 * Find the author ranking matching a collaborator name (case-insensitive),
 * used by the Author Productivity detail panel for in-place selection.
 */
export function resolveCollaboratorAuthor<T extends { displayName: string }>(
  rankings: readonly T[],
  collaboratorName: string
): T | undefined {
  const target = collaboratorName.toLowerCase();
  return rankings.find((r) => r.displayName.toLowerCase() === target);
}
