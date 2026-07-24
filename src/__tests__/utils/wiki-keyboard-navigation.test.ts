import { describe, it, expect } from 'vitest';
import { classifyWikiNavigationKey } from '@/utils/wiki-keyboard-navigation';

/** Minimal event-shape builder to keep test fixtures terse. */
function keyEvent(opts: { key: string; metaKey?: boolean; altKey?: boolean }): {
  key: string;
  metaKey: boolean;
  altKey: boolean;
} {
  return { key: opts.key, metaKey: opts.metaKey ?? false, altKey: opts.altKey ?? false };
}

describe('classifyWikiNavigationKey', () => {
  // ── macOS shortcuts: Cmd+[ / Cmd+] / Cmd+Left / Cmd+Right ──────────────
  describe('macOS (isMac = true)', () => {
    it('classifies Cmd+[ as back', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: '[', metaKey: true }), true)).toBe('back');
    });

    it('classifies Cmd+] as forward', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: ']', metaKey: true }), true)).toBe(
        'forward'
      );
    });

    it('classifies Cmd+ArrowLeft as back (alias for Cmd+[)', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'ArrowLeft', metaKey: true }), true)).toBe(
        'back'
      );
    });

    it('classifies Cmd+ArrowRight as forward (alias for Cmd+])', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'ArrowRight', metaKey: true }), true)).toBe(
        'forward'
      );
    });

    it('returns null for Cmd+[ without the meta modifier', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: '[' }), true)).toBeNull();
    });

    it('returns null for Cmd+] without the meta modifier', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: ']' }), true)).toBeNull();
    });

    it('returns null for Alt+ArrowLeft on macOS (macOS uses Cmd, not Alt)', () => {
      // This pins the platform exclusivity: Alt+Left on macOS must NOT trigger
      // back navigation, even though it would on Windows/Linux.
      expect(
        classifyWikiNavigationKey(keyEvent({ key: 'ArrowLeft', altKey: true }), true)
      ).toBeNull();
    });

    it('returns null for Alt+ArrowRight on macOS', () => {
      expect(
        classifyWikiNavigationKey(keyEvent({ key: 'ArrowRight', altKey: true }), true)
      ).toBeNull();
    });

    it('returns null for an unrelated key with Cmd held', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'a', metaKey: true }), true)).toBeNull();
    });

    it('returns null for Cmd+ArrowUp (only Left/Right are navigation)', () => {
      expect(
        classifyWikiNavigationKey(keyEvent({ key: 'ArrowUp', metaKey: true }), true)
      ).toBeNull();
    });
  });

  // ── Windows / Linux shortcuts: Alt+Left / Alt+Right ───────────────────
  describe('Windows / Linux (isMac = false)', () => {
    it('classifies Alt+ArrowLeft as back', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'ArrowLeft', altKey: true }), false)).toBe(
        'back'
      );
    });

    it('classifies Alt+ArrowRight as forward', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'ArrowRight', altKey: true }), false)).toBe(
        'forward'
      );
    });

    it('returns null for ArrowLeft without the alt modifier', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'ArrowLeft' }), false)).toBeNull();
    });

    it('returns null for ArrowRight without the alt modifier', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'ArrowRight' }), false)).toBeNull();
    });

    it('returns null for Alt+[ on Windows (Windows uses Alt+Arrow, not Alt+bracket)', () => {
      // Pins the platform exclusivity: Alt+[ on Windows must NOT trigger
      // back navigation, even though Cmd+[ does on macOS.
      expect(classifyWikiNavigationKey(keyEvent({ key: '[', altKey: true }), false)).toBeNull();
    });

    it('returns null for Cmd+ArrowLeft on Windows (Windows uses Alt, not Cmd)', () => {
      expect(
        classifyWikiNavigationKey(keyEvent({ key: 'ArrowLeft', metaKey: true }), false)
      ).toBeNull();
    });

    it('returns null for an unrelated key with Alt held', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: 'a', altKey: true }), false)).toBeNull();
    });

    it('returns null for Alt+ArrowDown (only Left/Right are navigation)', () => {
      expect(
        classifyWikiNavigationKey(keyEvent({ key: 'ArrowDown', altKey: true }), false)
      ).toBeNull();
    });
  });

  // ── Cross-platform edge cases ────────────────────────────────────────
  describe('cross-platform edge cases', () => {
    it('returns null for an empty-key event on either platform', () => {
      expect(classifyWikiNavigationKey(keyEvent({ key: '' }), true)).toBeNull();
      expect(classifyWikiNavigationKey(keyEvent({ key: '' }), false)).toBeNull();
    });

    it('is case-sensitive on the bracket keys (lowercase variants are not shortcuts)', () => {
      // The DOM fires `key: '['` for the bracket regardless of shift state,
      // but if some IME produced a different grapheme it should not match.
      expect(classifyWikiNavigationKey(keyEvent({ key: '【', metaKey: true }), true)).toBeNull();
    });

    it('returns null when both modifiers are held with an arrow key on macOS', () => {
      // Cmd+Alt+ArrowLeft: the macOS branch checks `metaKey` first, so this
      // still classifies as 'back'. This test documents that behavior so a
      // future change to require exclusive modifiers is caught.
      expect(
        classifyWikiNavigationKey(keyEvent({ key: 'ArrowLeft', metaKey: true, altKey: true }), true)
      ).toBe('back');
    });

    it('returns null when both modifiers are held with an arrow key on Windows', () => {
      // Cmd+Alt+ArrowLeft on Windows: the Windows branch checks `altKey`,
      // so this still classifies as 'back'. Documents the non-exclusive
      // modifier behavior.
      expect(
        classifyWikiNavigationKey(
          keyEvent({ key: 'ArrowLeft', metaKey: true, altKey: true }),
          false
        )
      ).toBe('back');
    });
  });
});
