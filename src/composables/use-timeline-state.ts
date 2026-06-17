import { ref } from 'vue';

/**
 * Persistent view-state for the Publication Timeline.
 *
 * Lifted out of the component so it survives navigation away and back
 * (e.g. deep-linking to the article list and returning). The bibliometric
 * KPI data itself lives in `useBibliometrics` (singleton); this composable
 * only holds the local *view* preferences.
 */

type TimelineChartMode = 'bars' | 'line' | 'stacked';

const yearFrom = ref<number | null>(null);
const yearTo = ref<number | null>(null);
const chartMode = ref<TimelineChartMode>('bars');
const showCumulative = ref(true);
const showCitations = ref(true);
const selectedYear = ref<number | null>(null);
const selectedJournals = ref<string[]>([]);
const sidebarCollapsed = ref(false);

export function useTimelineState() {
  function setRange(min: number, max: number): void {
    if (yearFrom.value === null) yearFrom.value = min;
    if (yearTo.value === null) yearTo.value = max;
  }

  function reset(min: number, max: number): void {
    yearFrom.value = min;
    yearTo.value = max;
    chartMode.value = 'bars';
    showCumulative.value = true;
    showCitations.value = true;
    selectedYear.value = null;
    selectedJournals.value = [];
    sidebarCollapsed.value = false;
  }

  return {
    yearFrom,
    yearTo,
    chartMode,
    showCumulative,
    showCitations,
    selectedYear,
    selectedJournals,
    sidebarCollapsed,
    setRange,
    reset,
  };
}
