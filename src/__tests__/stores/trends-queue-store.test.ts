import { describe, it, expect, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useTrendsQueueStore } from '@/stores/trends-queue';
import { MAX_QUEUE_SIZE } from '@/utils/google-trends';

describe('useTrendsQueueStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('starts with empty keywords and default time range', () => {
    const store = useTrendsQueueStore();
    expect(store.keywords).toEqual([]);
    expect(store.hasKeywords).toBe(false);
    expect(store.timeRangeId).toBe('5y');
    expect(store.halted).toBe(false);
    expect(store.collapsed).toBe(false);
    expect(store.revision).toBe(0);
  });

  describe('addKeyword', () => {
    it('adds a keyword', () => {
      const store = useTrendsQueueStore();
      expect(store.addKeyword('sugar tax')).toBe(true);
      expect(store.keywords).toEqual(['sugar tax']);
      expect(store.hasKeywords).toBe(true);
      expect(store.revision).toBe(1);
    });

    it('trims whitespace', () => {
      const store = useTrendsQueueStore();
      expect(store.addKeyword('  spaced  ')).toBe(true);
      expect(store.keywords).toEqual(['spaced']);
    });

    it('rejects empty string', () => {
      const store = useTrendsQueueStore();
      expect(store.addKeyword('')).toBe(false);
      expect(store.addKeyword('   ')).toBe(false);
      expect(store.keywords).toEqual([]);
      expect(store.revision).toBe(0);
    });

    it('rejects duplicates case-insensitively but keeps first casing', () => {
      const store = useTrendsQueueStore();
      store.addKeyword('Sugar');
      expect(store.addKeyword('sugar')).toBe(false);
      expect(store.addKeyword('SUGAR')).toBe(false);
      expect(store.keywords).toEqual(['Sugar']);
    });

    it('enforces max queue size', () => {
      const store = useTrendsQueueStore();
      for (let i = 0; i < MAX_QUEUE_SIZE; i++) {
        expect(store.addKeyword(`kw-${i}`)).toBe(true);
      }
      // Adding one more should fail
      expect(store.addKeyword('overflow')).toBe(false);
      expect(store.keywords.length).toBe(MAX_QUEUE_SIZE);
    });

    it('clears halted when adding', () => {
      const store = useTrendsQueueStore();
      store.haltQueue();
      expect(store.halted).toBe(true);
      store.addKeyword('new');
      expect(store.halted).toBe(false);
    });
  });

  describe('removeKeyword', () => {
    it('removes an existing keyword case-insensitively', () => {
      const store = useTrendsQueueStore();
      store.addKeyword('Sugar');
      store.addKeyword('Tax');
      expect(store.removeKeyword('sugar')).toBe(true);
      expect(store.keywords).toEqual(['Tax']);
    });

    it('returns false for unknown keyword', () => {
      const store = useTrendsQueueStore();
      store.addKeyword('Sugar');
      expect(store.removeKeyword('nonexistent')).toBe(false);
      expect(store.keywords).toEqual(['Sugar']);
    });

    it('clears halted when removing', () => {
      const store = useTrendsQueueStore();
      store.addKeyword('sugar');
      store.haltQueue();
      store.removeKeyword('sugar');
      expect(store.halted).toBe(false);
    });
  });

  describe('clearAll', () => {
    it('removes all keywords and clears halted', () => {
      const store = useTrendsQueueStore();
      store.addKeyword('a');
      store.addKeyword('b');
      store.haltQueue();
      const revBefore = store.revision;
      store.clearAll();
      expect(store.keywords).toEqual([]);
      expect(store.halted).toBe(false);
      expect(store.revision).toBe(revBefore + 1);
    });
  });

  describe('setTimeRange', () => {
    it('changes timeRangeId and clears halted', () => {
      const store = useTrendsQueueStore();
      store.haltQueue();
      store.setTimeRange('12m');
      expect(store.timeRangeId).toBe('12m');
      expect(store.halted).toBe(false);
    });
  });

  describe('setCustomRange', () => {
    it('sets custom range and switches timeRangeId', () => {
      const store = useTrendsQueueStore();
      const clamped = store.setCustomRange('2020-01-01', '2022-01-01');
      expect(store.timeRangeId).toBe('custom');
      expect(store.customStart).toBe('2020-01-01');
      expect(store.customEnd).toBe('2022-01-01');
      expect(typeof clamped).toBe('boolean');
    });
  });

  describe('setResearchRange', () => {
    it('sets research range', () => {
      const store = useTrendsQueueStore();
      store.setResearchRange(2010, 2020, 2018);
      expect(store.researchRange).not.toBeNull();
      expect(store.researchRange!.start).toBeDefined();
      expect(store.researchRange!.end).toBeDefined();
    });
  });

  describe('resolvedRange', () => {
    it('returns preset values for 5y default', () => {
      const store = useTrendsQueueStore();
      const r = store.resolvedRange;
      expect(r.label).toBeDefined();
      expect(r.apiTime).toBeDefined();
    });

    it('falls back when custom selected but dates missing', () => {
      const store = useTrendsQueueStore();
      store.setTimeRange('custom');
      // No customStart/customEnd set
      const r = store.resolvedRange;
      expect(r.label).toContain('Fallback');
    });

    it('uses custom range when valid dates provided', () => {
      const store = useTrendsQueueStore();
      store.setCustomRange('2020-01-01', '2022-01-01');
      const r = store.resolvedRange;
      expect(r.label).toBe('Custom Range');
      expect(r.apiTime).toContain('2020-01-01');
    });

    it('uses research range when set and selected', () => {
      const store = useTrendsQueueStore();
      store.setResearchRange(2010, 2020, 2018);
      store.setTimeRange('research');
      const r = store.resolvedRange;
      expect(r.label).toBe('Research Range');
    });

    it('falls back when research selected but range missing', () => {
      const store = useTrendsQueueStore();
      store.setTimeRange('research');
      const r = store.resolvedRange;
      expect(r.label).toContain('Fallback');
    });
  });

  describe('halt/resume', () => {
    it('haltQueue sets halted', () => {
      const store = useTrendsQueueStore();
      store.haltQueue();
      expect(store.halted).toBe(true);
    });

    it('resumeQueue clears halted and bumps revision', () => {
      const store = useTrendsQueueStore();
      store.haltQueue();
      const rev = store.revision;
      store.resumeQueue();
      expect(store.halted).toBe(false);
      expect(store.revision).toBe(rev + 1);
    });
  });

  describe('toggleCollapsed', () => {
    it('toggles collapse state', () => {
      const store = useTrendsQueueStore();
      expect(store.collapsed).toBe(false);
      store.toggleCollapsed();
      expect(store.collapsed).toBe(true);
      store.toggleCollapsed();
      expect(store.collapsed).toBe(false);
    });
  });

  describe('bumpRevision', () => {
    it('increments revision', () => {
      const store = useTrendsQueueStore();
      const rev = store.revision;
      store.bumpRevision();
      expect(store.revision).toBe(rev + 1);
    });
  });
});
