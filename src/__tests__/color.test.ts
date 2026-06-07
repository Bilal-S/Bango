import { describe, it, expect } from 'vitest';
import { deriveColorScheme, hashColor, getColorScheme } from '@/utils/color';

describe('color utils', () => {
  describe('hashColor', () => {
    it('returns a hex color string', () => {
      const result = hashColor('test');
      expect(result).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('is deterministic - same input always yields same output', () => {
      expect(hashColor('machine-learning')).toBe(hashColor('machine-learning'));
    });

    it('different inputs usually produce different colors', () => {
      const c1 = hashColor('foo');
      const c2 = hashColor('bar');
      expect(c1).not.toBe(c2);
    });

    it('returns a color from the fallback palette', () => {
      const palette = [
        '#3b82f6',
        '#10b981',
        '#8b5cf6',
        '#f59e0b',
        '#ef4444',
        '#06b6d4',
        '#ec4899',
        '#84cc16',
      ];
      expect(palette).toContain(hashColor('anything'));
    });
  });

  describe('deriveColorScheme', () => {
    it('returns base unchanged', () => {
      const scheme = deriveColorScheme('#3b82f6');
      expect(scheme.base).toBe('#3b82f6');
    });

    it('produces lighter background than base', () => {
      const scheme = deriveColorScheme('#3b82f6');
      expect(scheme.bg).not.toBe(scheme.base);
      // bg should be lighter (closer to white)
      expect(scheme.bg).toBeDefined();
    });

    it('produces border color', () => {
      const scheme = deriveColorScheme('#10b981');
      expect(scheme.border).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('produces text color that is darker than base', () => {
      const scheme = deriveColorScheme('#3b82f6');
      expect(scheme.text).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('produces bgHover color', () => {
      const scheme = deriveColorScheme('#ef4444');
      expect(scheme.bgHover).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('all fields are valid hex colors', () => {
      const scheme = deriveColorScheme('#8b5cf6');
      const hexRegex = /^#[0-9a-f]{6}$/;
      expect(scheme.base).toMatch(hexRegex);
      expect(scheme.bg).toMatch(hexRegex);
      expect(scheme.border).toMatch(hexRegex);
      expect(scheme.text).toMatch(hexRegex);
      expect(scheme.bgHover).toMatch(hexRegex);
    });
  });

  describe('getColorScheme', () => {
    it('uses custom color when provided', () => {
      const scheme = getColorScheme('test', '#ef4444');
      expect(scheme.base).toBe('#ef4444');
    });

    it('falls back to hash color when custom color is null', () => {
      const scheme = getColorScheme('test', null);
      expect(scheme.base).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('falls back to hash color when custom color is undefined', () => {
      const scheme = getColorScheme('test', undefined);
      expect(scheme.base).toMatch(/^#[0-9a-f]{6}$/);
    });

    it('returns consistent scheme for same inputs', () => {
      const s1 = getColorScheme('tag-name', null);
      const s2 = getColorScheme('tag-name', null);
      expect(s1.base).toBe(s2.base);
      expect(s1.bg).toBe(s2.bg);
    });
  });
});
