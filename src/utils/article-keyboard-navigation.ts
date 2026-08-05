/* Pure classification helpers for Articles view keyboard navigation.
 * Arrow-key → direction decision matrix, unit-testable without Vue/DOM.
 *
 * Two contexts:
 * 1. Detail panel open: bare ArrowLeft/ArrowRight → prev/next article.
 * 2. Table focused: bare ArrowUp/Down → row selection, ArrowLeft/Right → scroll.
 *
 * Only BARE arrow keys (no modifiers) are classified to avoid hijacking text
 * editing (Shift+Arrow to select, Ctrl/Cmd+Arrow words, Alt+Arrow wiki nav).
 */

/** Direction for article-detail prev/next: ArrowLeft→prev, ArrowRight→next. */
export type ArticleDetailNavDirection = 'prev' | 'next';

/** Direction for article-table: up/down→row, scroll-left/right→table chevrons. */
export type ArticleTableNavDirection = 'up' | 'down' | 'scroll-left' | 'scroll-right';

/** True when no keyboard modifier (Shift/Ctrl/Alt/Meta) is held. */
function noModifiersHeld(event: {
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): boolean {
  return !event.shiftKey && !event.ctrlKey && !event.altKey && !event.metaKey;
}

/** Classify bare ArrowLeft/ArrowRight as article-detail prev/next nav. */
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

/** Classify bare arrow keys as article-table nav/scroll direction. */
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
