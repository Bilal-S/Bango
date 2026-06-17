import { describe, it, expect } from 'vitest';
import { getTemporalColor, deriveColorScheme } from '@/utils/color';

describe('color utils (extended)', () => {
  describe('getTemporalColor', () => {
    it('returns gray for null year', () => {
      expect(getTemporalColor(null, 2010, 2020)).toBe('#cbd5e1');
    });
    it('returns gray for undefined year', () => {
      expect(getTemporalColor(undefined, 2010, 2020)).toBe('#cbd5e1');
    });
    it('returns start color when maxYear <= minYear', () => {
      expect(getTemporalColor(2015, 2020, 2020)).toBe('#56B4E9');
      expect(getTemporalColor(2015, 2020, 2019)).toBe('#56B4E9');
    });
    it('returns start color at minYear', () => {
      expect(getTemporalColor(2010, 2010, 2020)).toBe('#56b4e9');
    });
    it('returns end color at maxYear', () => {
      expect(getTemporalColor(2020, 2010, 2020)).toBe('#e69f00');
    });
    it('interpolates a mid value', () => {
      const mid = getTemporalColor(2015, 2010, 2020);
      expect(mid).toMatch(/^#[0-9a-f]{6}$/);
      // Mid should differ from both endpoints
      expect(mid).not.toBe('#56b4e9');
      expect(mid).not.toBe('#e69f00');
    });
    it('clamps year below min to start', () => {
      const before = getTemporalColor(2000, 2010, 2020);
      expect(before).toBe('#56b4e9');
    });
    it('clamps year above max to end', () => {
      const after = getTemporalColor(2030, 2010, 2020);
      expect(after).toBe('#e69f00');
    });
  });

  describe('deriveColorScheme edge cases', () => {
    it('returns base unchanged for black', () => {
      const s = deriveColorScheme('#000000');
      expect(s.base).toBe('#000000');
      expect(s.bg).toMatch(/^#[0-9a-f]{6}$/);
    });
    it('returns base unchanged for white', () => {
      const s = deriveColorScheme('#ffffff');
      expect(s.base).toBe('#ffffff');
    });
  });
});
