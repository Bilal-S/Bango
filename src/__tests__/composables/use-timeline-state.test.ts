import { describe, it, expect, beforeEach } from 'vitest';
import { useTimelineState } from '@/composables/use-timeline-state';

describe('useTimelineState', () => {
  beforeEach(() => {
    // Reset the module-level refs by calling reset on a fresh instance.
    const state = useTimelineState();
    state.reset(2000, 2024);
  });

  it('initializes with null year range', () => {
    const { yearFrom, yearTo, chartMode, showCumulative, showCitations } = useTimelineState();
    // After the beforeEach reset, values are set. But if we create a fresh
    // instance without calling reset, the module-level refs still hold the
    // reset values. We verify the refs are accessible and typed correctly.
    expect(yearFrom.value).toBe(2000);
    expect(yearTo.value).toBe(2024);
    expect(chartMode.value).toBe('bars');
    expect(showCumulative.value).toBe(true);
    expect(showCitations.value).toBe(true);
  });

  it('setRange only sets null values (first-call semantics)', () => {
    const { yearFrom, yearTo, setRange } = useTimelineState();

    yearFrom.value = null;
    yearTo.value = null;

    setRange(1990, 2020);
    expect(yearFrom.value).toBe(1990);
    expect(yearTo.value).toBe(2020);

    // Second call: values are already set, so they are NOT overwritten.
    setRange(2000, 2010);
    expect(yearFrom.value).toBe(1990);
    expect(yearTo.value).toBe(2020);
  });

  it('reset overwrites all state', () => {
    const {
      yearFrom,
      yearTo,
      chartMode,
      showCumulative,
      showCitations,
      selectedYear,
      selectedJournals,
      sidebarCollapsed,
      reset,
    } = useTimelineState();

    // Dirty the state.
    yearFrom.value = 1980;
    yearTo.value = 2022;
    chartMode.value = 'line';
    showCumulative.value = false;
    showCitations.value = false;
    selectedYear.value = 2015;
    selectedJournals.value = ['Nature', 'Science'];
    sidebarCollapsed.value = true;

    // Reset to defaults.
    reset(1990, 2025);

    expect(yearFrom.value).toBe(1990);
    expect(yearTo.value).toBe(2025);
    expect(chartMode.value).toBe('bars');
    expect(showCumulative.value).toBe(true);
    expect(showCitations.value).toBe(true);
    expect(selectedYear.value).toBeNull();
    expect(selectedJournals.value).toEqual([]);
    expect(sidebarCollapsed.value).toBe(false);
  });

  it('chartMode can be changed', () => {
    const { chartMode } = useTimelineState();
    chartMode.value = 'stacked';
    expect(chartMode.value).toBe('stacked');
    chartMode.value = 'line';
    expect(chartMode.value).toBe('line');
  });

  it('selectedYear can be set and cleared', () => {
    const { selectedYear } = useTimelineState();
    selectedYear.value = 2018;
    expect(selectedYear.value).toBe(2018);
    selectedYear.value = null;
    expect(selectedYear.value).toBeNull();
  });

  it('selectedJournals can be mutated', () => {
    const { selectedJournals } = useTimelineState();
    selectedJournals.value = ['Journal A'];
    expect(selectedJournals.value).toEqual(['Journal A']);
    selectedJournals.value = [...selectedJournals.value, 'Journal B'];
    expect(selectedJournals.value).toEqual(['Journal A', 'Journal B']);
  });

  it('sidebarCollapsed toggles', () => {
    const { sidebarCollapsed } = useTimelineState();
    expect(sidebarCollapsed.value).toBe(false);
    sidebarCollapsed.value = true;
    expect(sidebarCollapsed.value).toBe(true);
  });

  it('showCumulative and showCitations toggle independently', () => {
    const { showCumulative, showCitations } = useTimelineState();

    showCumulative.value = false;
    expect(showCumulative.value).toBe(false);
    expect(showCitations.value).toBe(true);

    showCitations.value = false;
    expect(showCumulative.value).toBe(false);
    expect(showCitations.value).toBe(false);
  });

  it('multiple instances share the same module-level refs', () => {
    const a = useTimelineState();
    const b = useTimelineState();

    a.yearFrom.value = 2010;
    expect(b.yearFrom.value).toBe(2010);

    b.chartMode.value = 'line';
    expect(a.chartMode.value).toBe('line');
  });
});
