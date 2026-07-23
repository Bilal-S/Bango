/**
 * Pure helpers for the Bibliometrics -> Article list cross-linking feature.
 *
 * Extracted from the consuming views (`article-list.vue`,
 * `biblio-authors.vue`) so the routing/lookup logic is unit-testable without
 * mounting the full Vue components (which require Tauri, Pinia, router, and
 * chart-library mocking).
 */

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
};

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
