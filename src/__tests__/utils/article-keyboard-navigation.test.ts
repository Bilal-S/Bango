import { describe, it, expect } from 'vitest';
import {
  classifyArticleDetailArrowKey,
  classifyArticleTableArrowKey,
} from '@/utils/article-keyboard-navigation';

/** Minimal event-shape builder to keep test fixtures terse. All modifiers
 *  default to false so the common case (bare arrow key) reads cleanly. */
function keyEvent(opts: {
  key: string;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
}): {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
} {
  return {
    key: opts.key,
    shiftKey: opts.shiftKey ?? false,
    ctrlKey: opts.ctrlKey ?? false,
    altKey: opts.altKey ?? false,
    metaKey: opts.metaKey ?? false,
  };
}

describe('article-keyboard-navigation', () => {
  // ── Detail-panel classifier: ArrowLeft / ArrowRight ───────────────
  describe('classifyArticleDetailArrowKey', () => {
    it('classifies bare ArrowLeft as prev', () => {
      expect(classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowLeft' }))).toBe('prev');
    });

    it('classifies bare ArrowRight as next', () => {
      expect(classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowRight' }))).toBe('next');
    });

    it('returns null for bare ArrowUp / ArrowDown (detail only uses Left/Right)', () => {
      expect(classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowUp' }))).toBeNull();
      expect(classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowDown' }))).toBeNull();
    });

    it('returns null when Shift is held (preserve text selection)', () => {
      expect(
        classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowLeft', shiftKey: true }))
      ).toBeNull();
      expect(
        classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowRight', shiftKey: true }))
      ).toBeNull();
    });

    it('returns null when Ctrl is held (preserve word-jump shortcut)', () => {
      expect(
        classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowLeft', ctrlKey: true }))
      ).toBeNull();
    });

    it('returns null when Alt is held (no collision with wiki Alt+Arrow nav)', () => {
      expect(
        classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowLeft', altKey: true }))
      ).toBeNull();
    });

    it('returns null when Meta (Cmd) is held (no collision with macOS Cmd+Arrow)', () => {
      expect(
        classifyArticleDetailArrowKey(keyEvent({ key: 'ArrowRight', metaKey: true }))
      ).toBeNull();
    });

    it('returns null for unrelated keys', () => {
      expect(classifyArticleDetailArrowKey(keyEvent({ key: 'Enter' }))).toBeNull();
      expect(classifyArticleDetailArrowKey(keyEvent({ key: 'a' }))).toBeNull();
      expect(classifyArticleDetailArrowKey(keyEvent({ key: ' ' }))).toBeNull();
    });
  });

  // ── Table classifier: ArrowUp / ArrowDown / ArrowLeft / ArrowRight ─
  describe('classifyArticleTableArrowKey', () => {
    it('classifies bare ArrowUp as up (row navigation)', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'ArrowUp' }))).toBe('up');
    });

    it('classifies bare ArrowDown as down (row navigation)', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'ArrowDown' }))).toBe('down');
    });

    it('classifies bare ArrowLeft as scroll-left (chevron)', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'ArrowLeft' }))).toBe('scroll-left');
    });

    it('classifies bare ArrowRight as scroll-right (chevron)', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'ArrowRight' }))).toBe('scroll-right');
    });

    it('returns null when Shift is held (preserve text selection)', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'ArrowUp', shiftKey: true }))).toBeNull();
      expect(
        classifyArticleTableArrowKey(keyEvent({ key: 'ArrowDown', shiftKey: true }))
      ).toBeNull();
    });

    it('returns null when Ctrl is held', () => {
      expect(
        classifyArticleTableArrowKey(keyEvent({ key: 'ArrowLeft', ctrlKey: true }))
      ).toBeNull();
    });

    it('returns null when Alt is held (no collision with wiki Alt+Arrow)', () => {
      expect(
        classifyArticleTableArrowKey(keyEvent({ key: 'ArrowRight', altKey: true }))
      ).toBeNull();
    });

    it('returns null when Meta is held', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'ArrowUp', metaKey: true }))).toBeNull();
    });

    it('returns null for unrelated keys', () => {
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'PageDown' }))).toBeNull();
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'Home' }))).toBeNull();
      expect(classifyArticleTableArrowKey(keyEvent({ key: 'Tab' }))).toBeNull();
    });
  });
});
