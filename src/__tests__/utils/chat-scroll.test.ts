/**
 * Unit tests for the chat scroll-anchoring helper (`src/utils/chat-scroll.ts`).
 *
 * jsdom has no layout, so `getBoundingClientRect` is stubbed per element and
 * `scrollTo` is replaced with a spy; the assertions pin the arithmetic
 * contract used by the chat view's citation-results scroll (pin the claim
 * message to the top of the scroll viewport).
 */

import { describe, it, expect, vi } from 'vitest';
import { scrollAnchorToContainerTop } from '@/utils/chat-scroll';

/** Build an element with a stubbed rect top + scrollTop and a spied scrollTo. */
function makeEl(rectTop: number, scrollTop = 0): HTMLElement {
  const el = document.createElement('div');
  el.getBoundingClientRect = () => ({ top: rectTop }) as DOMRect;
  Object.defineProperty(el, 'scrollTop', { value: scrollTop, writable: true });
  Object.defineProperty(el, 'scrollTo', { value: vi.fn(), writable: true });
  return el;
}

describe('scrollAnchorToContainerTop', () => {
  it('scrolls_so_anchor_top_meets_container_top', () => {
    // Viewport top at y=100 (scrolled down 50px); anchor sits 150px below it.
    const container = makeEl(100, 50);
    const anchor = makeEl(250);
    scrollAnchorToContainerTop(container, anchor);
    expect(container.scrollTo).toHaveBeenCalledTimes(1);
    expect(container.scrollTo).toHaveBeenCalledWith({ top: 200, behavior: 'smooth' });
  });

  it('clamps_to_zero_when_anchor_is_above_container_top', () => {
    // Anchor 60px above the visible top with scrollTop 10 → target -50 → 0.
    const container = makeEl(100, 10);
    const anchor = makeEl(40);
    scrollAnchorToContainerTop(container, anchor);
    expect(container.scrollTo).toHaveBeenCalledWith({ top: 0, behavior: 'smooth' });
  });

  it('no_op_when_already_pinned', () => {
    // Anchor top == container top: nothing to do, no scroll call.
    const container = makeEl(100, 300);
    const anchor = makeEl(100);
    scrollAnchorToContainerTop(container, anchor);
    expect(container.scrollTo).not.toHaveBeenCalled();
  });

  it('instant_behavior_when_smooth_false', () => {
    const container = makeEl(0, 0);
    const anchor = makeEl(320);
    scrollAnchorToContainerTop(container, anchor, false);
    expect(container.scrollTo).toHaveBeenCalledWith({ top: 320, behavior: 'auto' });
  });
});
