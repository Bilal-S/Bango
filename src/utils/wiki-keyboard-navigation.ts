/* Pure classification helper for Wiki view back/forward keyboard shortcuts.
 * macOS: Cmd+[/] (+Cmd+Left/Right); Windows/Linux: Alt+Left/Right.
 * Returns `'back' | 'forward' | null` - caller owns addEventListener + preventDefault.
 * Extracted from `wiki-view.vue` for exhaustive unit-testing without Vue/DOM. */

/** Direction the pressed shortcut maps to, or `null` when no match. */
export type WikiNavDirection = 'back' | 'forward';

/** Classify a keyboard event as a Wiki back/forward nav shortcut for the given platform. */
export function classifyWikiNavigationKey(
  event: { key: string; metaKey: boolean; altKey: boolean },
  isMac: boolean
): WikiNavDirection | null {
  // macOS: Cmd+[/] (+ Cmd+Left/Right)
  if (isMac) {
    if (event.metaKey && (event.key === '[' || event.key === 'ArrowLeft')) {
      return 'back';
    }
    if (event.metaKey && (event.key === ']' || event.key === 'ArrowRight')) {
      return 'forward';
    }
    return null;
  }
  // Windows/Linux: Alt+Left/Right
  if (event.altKey && event.key === 'ArrowLeft') {
    return 'back';
  }
  if (event.altKey && event.key === 'ArrowRight') {
    return 'forward';
  }
  return null;
}
