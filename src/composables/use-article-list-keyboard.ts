import { onActivated, onDeactivated, onUnmounted, type ComputedRef, type Ref } from 'vue';
import {
  classifyArticleDetailArrowKey,
  classifyArticleTableArrowKey,
} from '@/utils/article-keyboard-navigation';

/** Minimal exposed surface of `article-table.vue` the scroll shortcuts need. */
export interface ArticleTableHandle {
  canScrollLeft: boolean;
  canScrollRight: boolean;
  scrollTable: (direction: 'left' | 'right') => void;
}

export interface ArticleListKeyboardDeps {
  showDetail: Ref<boolean>;
  selectedArticle: Ref<{ id: string } | null>;
  activeStatusTab: Ref<string>;
  hasPrevious: ComputedRef<boolean>;
  hasNext: ComputedRef<boolean>;
  navigatePrev: () => Promise<void>;
  navigateNext: () => Promise<void>;
  articleTableRef: Ref<ArticleTableHandle | null>;
}

/**
 * Keyboard navigation for the Articles view (refactor1 T4.2): context-dependent
 * arrow-key shortcuts.
 * Detail panel OPEN: ArrowLeft/Right -> prev/next article (reuses `navigatePrev`
 * / `navigateNext` including cross-page behavior).
 * Detail panel CLOSED (table focused): ArrowUp/Down -> select prev/next row;
 * ArrowLeft/Right -> simulate horizontal scroll chevrons.
 *
 * The listener is wired on `onActivated`, removed on `onDeactivated` because the
 * view is keep-alive cached: `onMounted` fires once for component lifetime,
 * so a listener there would fire while user is on another view.
 */
export function useArticleListKeyboard(deps: ArticleListKeyboardDeps) {
  const { showDetail, selectedArticle, activeStatusTab, hasPrevious, hasNext, articleTableRef } =
    deps;
  const { navigatePrev, navigateNext } = deps;

  /** True when `target` is an editable field the shortcuts must never hijack. */
  function isTypingTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null;
    if (!el) return false;
    return (
      el.tagName === 'INPUT' ||
      el.tagName === 'TEXTAREA' ||
      el.tagName === 'SELECT' ||
      el.isContentEditable
    );
  }

  /** True when the current tab owns an article table (not References / Search). */
  function tableIsVisible(): boolean {
    const tab = activeStatusTab.value;
    return tab !== 'references' && tab !== 'search';
  }

  /** Detail-panel arrows: prev/next article navigation. */
  function handleDetailArrows(e: KeyboardEvent): void {
    const dir = classifyArticleDetailArrowKey(e);
    if (dir === 'prev' && hasPrevious.value) {
      e.preventDefault();
      void navigatePrev();
    } else if (dir === 'next' && hasNext.value) {
      e.preventDefault();
      void navigateNext();
    }
  }

  /** Table arrows: row selection + horizontal scroll chevrons. */
  function handleTableArrows(e: KeyboardEvent): void {
    const dir = classifyArticleTableArrowKey(e);
    if (!dir) return;
    if (dir === 'up' && hasPrevious.value) {
      e.preventDefault();
      void navigatePrev();
    } else if (dir === 'down' && hasNext.value) {
      e.preventDefault();
      void navigateNext();
    } else if (dir === 'scroll-left' && articleTableRef.value?.canScrollLeft) {
      e.preventDefault();
      articleTableRef.value.scrollTable('left');
    } else if (dir === 'scroll-right' && articleTableRef.value?.canScrollRight) {
      e.preventDefault();
      articleTableRef.value.scrollTable('right');
    }
  }

  function onKeyDown(e: KeyboardEvent): void {
    // Never hijack typing in inputs / textareas / contenteditable / selects
    // (filter panel, toolbar search, notes, tags, bulk dialogs).
    if (isTypingTarget(e.target)) return;

    // Detail-panel arrows: prev/next article.
    if (showDetail.value && selectedArticle.value) {
      handleDetailArrows(e);
      return;
    }

    // Table arrows: only when the table is visible and the detail panel is closed.
    if (!tableIsVisible() || showDetail.value) return;
    handleTableArrows(e);
  }

  // Activate / deactivate the listener with the keep-alive lifecycle so the
  // shortcuts only fire while the Articles view is actually active.
  onActivated(() => {
    window.addEventListener('keydown', onKeyDown);
  });
  onDeactivated(() => {
    window.removeEventListener('keydown', onKeyDown);
  });
  // Also guard the non-cached path: if keep-alive is ever removed, the listener
  // should still be cleaned up on unmount.
  onUnmounted(() => {
    window.removeEventListener('keydown', onKeyDown);
  });

  return { onKeyDown };
}
