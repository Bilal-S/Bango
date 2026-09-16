/**
 * Chat scroll anchoring.
 *
 * Pure DOM helpers (no Vue reactivity) used by the chat view to pin a
 * message wrapper to the top of its scroll viewport. Container-relative
 * arithmetic is used instead of `Element.scrollIntoView` because the chat
 * scroll container sits inside other scrollable ancestors (the app shell)
 * and `scrollIntoView({ block: 'start' })` would scroll those too.
 */

/** Scroll `container` so `anchor`'s top edge touches the container's visible
 * top edge. No-op when the anchor is already pinned (delta === 0). Negative
 * targets clamp to 0 (anchor sits above the container's current top). */
export function scrollAnchorToContainerTop(
  container: HTMLElement,
  anchor: HTMLElement,
  smooth = true
): void {
  const delta = anchor.getBoundingClientRect().top - container.getBoundingClientRect().top;
  if (delta === 0) return;
  const target = Math.max(0, container.scrollTop + delta);
  container.scrollTo({ top: target, behavior: smooth ? 'smooth' : 'auto' });
}
