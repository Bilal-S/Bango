/**
 * Pure classification helpers for the Articles view keyboard navigation.
 * Extracted so the arrow-key -> direction decision matrix is exhaustively
 * unit-testable without Vue or DOM dependencies, mirroring the
 * `wiki-keyboard-navigation.ts` pattern.
 *
 * Two distinct contexts are supported:
 *
 * 1. **Article detail panel open** - bare `ArrowLeft` / `ArrowRight` move to
 *    the previous / next article (reusing `useArticleSearch.navigatePrev` /
 *    `navigateNext`).
 * 2. **Article table focused (detail closed)** - bare `ArrowUp` / `ArrowDown`
 *    move the selection to the previous / next row, and bare `ArrowLeft` /
 *    `ArrowRight` trigger the horizontal-scroll chevrons that flank the table.
 *
 * Both classifiers are intentionally pure: they read only the keyboard-event
 * shape (`key` + modifier flags) and return a direction or `null`. The calling
 * component owns the `addEventListener` lifecycle, the focus/typing guards,
 * and the `preventDefault` + navigation invocation.
 *
 * Only BARE arrow keys (no modifier keys) are classified so the shortcuts never
 * hijack text editing (Shift+Arrow to select, Ctrl/Cmd+Arrow to jump words,
 * Alt+Arrow wiki navigation on other views, etc.).
 */

/**
 * Direction for article-detail prev/next navigation.
 * - `'prev'` maps to `ArrowLeft`.
 * - `'next'` maps to `ArrowRight`.
 */
export type ArticleDetailNavDirection = 'prev' | 'next';

/**
 * Direction for article-table navigation / horizontal scroll.
 * - `'up'` / `'down'` map to row navigation.
 * - `'scroll-left'` / `'scroll-right'` map to the table's flank chevrons.
 */
export type ArticleTableNavDirection = 'up' | 'down' | 'scroll-left' | 'scroll-right';

/**
 * Returns true when no keyboard modifier (Shift / Ctrl / Alt / Meta) is held.
 * Used by both classifiers to enforce bare-arrow-key matching so the
 * shortcuts do not collide with text-editing or platform shortcuts.
 *
 * @internal
 */
function noModifiersHeld(event: {
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): boolean {
  return !event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey;
}

/**
 * Decide whether a keyboard event matches an article-detail prev/next
 * navigation arrow (bare `ArrowLeft` / `ArrowRight`).
 *
 * @param event - the DOM `KeyboardEvent` (only `key` + modifier flags are read)
 * @returns `'prev'`, `'next'`, or `null` when the event does not match
 */
export function classifyArticleDetailArrowKey(event: {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): ArticleDetailNavDirection | null {
  if (!noModifiersHeld(event)) return null;
  if (event.key === 'ArrowLeft') return 'prev';
  if (event.key === 'ArrowRight') return 'next';
  return null;
}

/**
 * Decide whether a keyboard event matches an article-table navigation /
 * horizontal-scroll arrow (bare `ArrowUp` / `ArrowDown` / `ArrowLeft` /
 * `ArrowRight`).
 *
 * @param event - the DOM `KeyboardEvent` (only `key` + modifier flags are read)
 * @returns the matched direction, or `null` when the event does not match
 */
export function classifyArticleTableArrowKey(event: {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): ArticleTableNavDirection | null {
  if (!noModifiersHeld(event)) return null;
  switch (event.key) {
    case 'ArrowUp':
      return 'up';
    case 'ArrowDown':
      return 'down';
    case 'ArrowLeft':
      return 'scroll-left';
    case 'ArrowRight':
      return 'scroll-right';
    default:
      return null;
  }
}
