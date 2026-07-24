/**
 * Pure classification helper for the Wiki view's back/forward keyboard
 * shortcuts. Extracted from `wiki-view.vue`'s `onKeyDown` handler so the
 * platform/key/modifier decision matrix is exhaustively unit-testable
 * without Vue or DOM dependencies.
 *
 * Shortcut scheme (browser parity):
 * - macOS: `Cmd+[` / `Cmd+]` (also `Cmd+Left` / `Cmd+Right`).
 * - Windows/Linux: `Alt+Left` / `Alt+Right`.
 *
 * The classification is intentionally pure: it returns `'back' | 'forward' | null`
 * and never touches the DOM. The calling component owns the
 * `addEventListener` / `removeEventListener` lifecycle and decides whether to
 * `preventDefault()` + invoke its nav history based on the result + the
 * component state (edit mode, active tab, current selection).
 */

/** Direction the pressed shortcut maps to, or `null` when no match. */
export type WikiNavDirection = 'back' | 'forward';

/**
 * Decide whether a keyboard event matches a Wiki back/forward navigation
 * shortcut for the given platform.
 *
 * @param event - the DOM `KeyboardEvent` (only `key`, `metaKey`, `altKey` are read)
 * @param isMac - whether the platform uses Cmd-based shortcuts (pass the
 *   result of {@link isMacPlatform} from `utils/platform.ts`)
 * @returns `'back'`, `'forward'`, or `null` when the event does not match
 *   any navigation shortcut for the platform
 */
export function classifyWikiNavigationKey(
  event: { key: string; metaKey: boolean; altKey: boolean },
  isMac: boolean
): WikiNavDirection | null {
  // macOS: Cmd+[ (back) / Cmd+] (forward), plus Cmd+Left / Cmd+Right.
  if (isMac) {
    if (event.metaKey && (event.key === '[' || event.key === 'ArrowLeft')) {
      return 'back';
    }
    if (event.metaKey && (event.key === ']' || event.key === 'ArrowRight')) {
      return 'forward';
    }
    return null;
  }
  // Windows / Linux: Alt+Left (back) / Alt+Right (forward).
  if (event.altKey && event.key === 'ArrowLeft') {
    return 'back';
  }
  if (event.altKey && event.key === 'ArrowRight') {
    return 'forward';
  }
  return null;
}
