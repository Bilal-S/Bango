<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue';
import { useRouter } from 'vue-router';
import VueApexCharts from 'vue3-apexcharts';
import type { ApexOptions, ApexYAxis } from 'apexcharts';
import { useBibliometrics } from '@/composables/use-bibliometrics';
import { useTimelineState } from '@/composables/use-timeline-state';
import { useViewport } from '@/composables/use-viewport';
import { tauriCommand } from '@/composables/use-tauri-command';
import JournalInfoCard from '@/components/journal-info-card.vue';

const router = useRouter();
const { kpis, loading, fetchKpis } = useBibliometrics();
const state = useTimelineState();
const { height: viewportHeight } = useViewport();

// ── Journal Info Card selection ────────────────────────────────
const selectedJournalKey = ref<string | null>(null);
const selectedJournalIndexId = ref<string | null>(null);

// ── Year range (persistent via state) ──────────────────────────
const allYears = computed(() => kpis.value.pubsByYear.map((yc) => yc.year));
const rangeMin = computed(() => allYears.value[0] ?? new Date().getFullYear());
const rangeMax = computed(
  () => allYears.value[allYears.value.length - 1] ?? new Date().getFullYear()
);

// Re-sync range bounds whenever the underlying dataset changes year span
// (e.g. after a re-normalization). Not immediate — initial range is set
// in onMounted after the first fetchKpis() call completes.
watch([rangeMin, rangeMax, () => kpis.value.includedCount], ([mn, mx, count]) => {
  if (count > 0) state.setRange(mn, mx);
});

const yearFrom = computed({
  get: () => state.yearFrom.value ?? rangeMin.value,
  set: (v) => {
    state.yearFrom.value = v;
  },
});
const yearTo = computed({
  get: () => state.yearTo.value ?? rangeMax.value,
  set: (v) => {
    state.yearTo.value = v;
  },
});

// Dual-handle slider: clamp so handles never cross
watch(yearFrom, (v) => {
  if (v > yearTo.value) state.yearFrom.value = yearTo.value;
});
watch(yearTo, (v) => {
  if (v < yearFrom.value) state.yearTo.value = yearFrom.value;
});

const filteredPubs = computed(() =>
  kpis.value.pubsByYear.filter((yc) => yc.year >= yearFrom.value && yc.year <= yearTo.value)
);

const filteredCitations = computed(() =>
  kpis.value.citationsByYear.filter((yc) => yc.year >= yearFrom.value && yc.year <= yearTo.value)
);

// ── Cumulative (range-local running sum) ───────────────────────
const cumulativeSeries = computed(() => {
  let running = 0;
  return filteredPubs.value.map((yc) => ({ year: yc.year, total: (running += yc.count) }));
});

// ── Journal indexing (pre-indexed for O(1) lookups) ────────────
const journalByYearIndex = computed(() => {
  const m = new Map<number, Map<string, { count: number; journalIndexId: string | null }>>();
  for (const jy of kpis.value.journalDistribution) {
    if (jy.year < yearFrom.value || jy.year > yearTo.value) continue;
    const inner = m.get(jy.year) ?? new Map();
    const existing = inner.get(jy.journal);
    const count = (existing?.count ?? 0) + jy.count;
    inner.set(jy.journal, { count, journalIndexId: jy.journalIndexId });
    m.set(jy.year, inner);
  }
  return m;
});

const topJournals = computed(() => {
  const totals = new Map<string, number>();
  for (const inner of journalByYearIndex.value.values()) {
    for (const [journal, { count }] of inner) {
      totals.set(journal, (totals.get(journal) ?? 0) + count);
    }
  }
  return [...totals.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 10)
    .map(([journal]) => journal);
});

const visibleJournals = computed(() =>
  state.selectedJournals.value.length > 0 ? state.selectedJournals.value : topJournals.value
);

// ── KPI strip ──────────────────────────────────────────────────
const totalInRange = computed(() => filteredPubs.value.reduce((s, yc) => s + yc.count, 0));
const peakEntry = computed(() => {
  if (filteredPubs.value.length === 0) return null;
  return filteredPubs.value.reduce(
    (max, yc) => (yc.count > max.count ? yc : max),
    filteredPubs.value[0]!
  );
});
const avgPerYear = computed(() =>
  filteredPubs.value.length > 0 ? (totalInRange.value / filteredPubs.value.length).toFixed(1) : '—'
);

// ── ApexCharts series & options ────────────────────────────────
const chartRef = ref<InstanceType<typeof VueApexCharts> | null>(null);

// ApexCharts requires a NUMERIC pixel height. We base the chart heights on the
// viewport so the charts fill the available space without needing ResizeObserver
// (which had timing issues with conditional v-if rendering). `viewportHeight`
// is a reactive ref from useViewport so charts recompute on resize.
const primaryChartHeight = computed(() => Math.max(280, Math.floor(viewportHeight.value * 0.42)));
const secondaryChartHeight = computed(() => Math.max(140, Math.floor(viewportHeight.value * 0.22)));

/**
 * Minimum viewport height (px) at which the secondary "Top Journals" chart is
 * worth showing. Below this, vertical space is too tight for two charts, so we
 * hide the secondary chart and let the primary chart expand to fill the area.
 *
 * Space budget at the cutoff: dashboard header (~60px) + KPI strip (~80px) +
 * primary chart (~42% of viewport ≈ 294px @ 700) + secondary chart (256px) +
 * paddings/gaps. 700px keeps both charts usable; below it the primary chart
 * would be crushed.
 */
const SECONDARY_CHART_MIN_VIEWPORT_HEIGHT = 700;

const showSecondaryChart = computed(
  () => viewportHeight.value >= SECONDARY_CHART_MIN_VIEWPORT_HEIGHT
);

const OKABE_ITO_10 = [
  '#E69F00',
  '#56B4E9',
  '#009E73',
  '#F0E442',
  '#0072B2',
  '#D55E00',
  '#CC79A7',
  '#33BBEE',
  '#EE7733',
  '#999999',
];
const OTHER_COLOR = '#c7c4d8';

const xCategories = computed(() => filteredPubs.value.map((yc) => String(yc.year)));

/**
 * Shorten journal names for legend display by stripping common leading
 * articles/prefixes (JOURNAL OF, ZEITSCHRIFT FUR, etc.), then truncating
 * to 25 characters. The full name is preserved for tooltips and data lookups.
 */
function normalizeJournalName(name: string): string {
  const cleaned = name
    .replace(/^(THE|A|AN)\s+/i, '')
    .replace(
      /^(JOURNAL OF|ZEITSCHRIFT FUR|REVISTA DE|ANNALES DE|ARCHIVES OF|BULLETIN OF|PROCEEDINGS OF|INTERNATIONAL JOURNAL OF|ACTA)\s+/i,
      ''
    )
    .trim();
  return cleaned.length > 25 ? cleaned.slice(0, 25) + '…' : cleaned;
}

/** Map from normalized (legend) name → full journal name, for tooltip lookup. */
const journalFullNameMap = computed(() => {
  const m = new Map<string, string>();
  for (const j of topJournals.value) {
    m.set(normalizeJournalName(j), j);
  }
  m.set('Other', 'Other');
  return m;
});

/** Build the series array depending on chart mode. */
const chartSeries = computed<ApexMultiAxisSeries[]>(() => {
  const years = xCategories.value;

  if (state.chartMode.value === 'stacked') {
    // One bar series per visible journal (aligned by year), plus "Other".
    const series: ApexMultiAxisSeries[] = visibleJournals.value.map((j, idx) => ({
      name: normalizeJournalName(j),
      type: 'bar',
      data: years.map((_, i) => {
        const yc = filteredPubs.value[i];
        if (!yc) return 0;
        return journalByYearIndex.value.get(yc.year)?.get(j)?.count ?? 0;
      }),
      color: OKABE_ITO_10[idx % OKABE_ITO_10.length],
    }));
    // "Other" bucket
    series.push({
      name: 'Other',
      type: 'bar',
      data: years.map((_, i) => {
        const yc = filteredPubs.value[i];
        if (!yc) return 0;
        const inner = journalByYearIndex.value.get(yc.year);
        if (!inner) return 0;
        let otherCount = 0;
        for (const [journal, { count }] of inner) {
          if (!visibleJournals.value.includes(journal)) otherCount += count;
        }
        return otherCount;
      }),
      color: OTHER_COLOR,
    });
    return series;
  }

  // Bars or Line mode: single publication-count series
  return [
    {
      name: 'Publications',
      type: state.chartMode.value === 'line' ? 'line' : 'bar',
      data: filteredPubs.value.map((yc) => yc.count),
      color: '#f59e0b',
    },
  ];
});

interface ApexMultiAxisSeries {
  name: string;
  type: 'bar' | 'line';
  data: number[];
  color?: string;
}

const chartOptions = computed<ApexOptions>(() => {
  const isStacked = state.chartMode.value === 'stacked';
  const hasOverlay = state.chartMode.value !== 'stacked';

  const yaxis: ApexYAxis[] = [
    {
      seriesName: 'Publications',
      title: { text: 'Publications', style: { color: '#475569' } },
      labels: { style: { colors: '#475569' } },
    },
  ];

  // Add cumulative + citations as overlay line series (own axes) in bars/line mode
  const series = [...chartSeries.value];
  if (hasOverlay) {
    if (state.showCumulative.value) {
      series.push({
        name: 'Cumulative',
        type: 'line',
        data: cumulativeSeries.value.map((c) => c.total),
        color: '#6366f1',
      });
      yaxis.push({
        seriesName: 'Cumulative',
        opposite: true,
        title: { text: 'Cumulative', style: { color: '#6366f1' } },
        labels: { style: { colors: '#6366f1' } },
      });
    }
    if (state.showCitations.value) {
      series.push({
        name: 'Citations',
        type: 'line',
        data: xCategories.value.map((_, i) => {
          const yc = filteredCitations.value[i];
          return yc ? yc.count : 0;
        }),
        color: '#64748b',
      });
      yaxis.push({
        seriesName: 'Citations',
        opposite: true,
        title: { text: 'Citations', style: { color: '#64748b' } },
        labels: { style: { colors: '#64748b' } },
      });
    }
  }

  return {
    chart: {
      type: 'line',
      stacked: isStacked,
      height: 320,
      toolbar: {
        show: false,
      },
      animations: { enabled: false },
      fontFamily: 'inherit',
      background: 'transparent',
      events: {
        click: handleDataPointClick as unknown as (...args: unknown[]) => void,
        legendClick: handleLegendClick as unknown as (...args: unknown[]) => void,
      },
    },
    plotOptions: {
      bar: {
        columnWidth: '70%',
        borderRadius: 4,
        borderRadiusApplication: 'end',
      },
    },
    fill: {
      type: state.chartMode.value === 'bars' ? 'gradient' : 'solid',
      gradient:
        state.chartMode.value === 'bars'
          ? {
              shade: 'light',
              type: 'vertical',
              shadeIntensity: 0.5,
              gradientToColors: ['#fbbf24'],
              inverseColors: false,
              opacityFrom: 0.95,
              opacityTo: 0.75,
              stops: [0, 100],
            }
          : undefined,
    },
    colors: series.map((s) => s.color ?? '#f59e0b'),
    series,
    stroke: {
      width: series.map((s) => (s.type === 'line' ? 2 : 0)),
      curve: 'straight',
      dashArray: series.map((s) => (s.name === 'Citations' ? 4 : 0)),
    },
    dataLabels: { enabled: false },
    xaxis: {
      categories: xCategories.value,
      labels: {
        rotate: -45,
        rotateAlways: xCategories.value.length > 8,
        style: { colors: '#94a3b8', fontSize: '10px' },
      },
      axisBorder: { show: false },
      axisTicks: { show: false },
    },
    yaxis,
    legend: {
      show: isStacked || hasOverlay,
      position: 'bottom',
      fontSize: '11px',
      markers: { size: 5 },
      onItemHover: {
        highlightDataSeries: true,
      },
      onItemClick: {
        toggleDataSeries: false,
      },
    },
    tooltip: {
      shared: !isStacked,
      intersect: false,
      theme: 'light',
      ...(isStacked
        ? {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            custom: ({ series: s, dataPointIndex, w }: any) => {
              const year = xCategories.value[dataPointIndex] ?? '';
              let rows = '';
              s.forEach((vals: number[], si: number) => {
                const val = vals[dataPointIndex] ?? 0;
                if (val > 0) {
                  const seriesName = w.config.series[si]?.name ?? '';
                  const fullName = journalFullNameMap.value.get(seriesName) ?? seriesName;
                  rows += `<div class="apexcharts-tooltip-series-group"><span class="apexcharts-tooltip-marker"></span><span class="apexcharts-tooltip-text"><span class="apexcharts-tooltip-y-group"><span class="apexcharts-tooltip-text-y-label">${fullName}</span><span class="apexcharts-tooltip-text-y-value">: <strong>${val}</strong></span></span></span></div>`;
                }
              });
              return `<div class="apexcharts-tooltip-title">${year}</div>${rows}`;
            },
          }
        : {
            y: {
              formatter: (val: number) => {
                if (val === undefined) return '';
                return String(val);
              },
            },
          }),
    },
    grid: {
      borderColor: '#f1f5f9',
      strokeDashArray: 3,
      xaxis: { lines: { show: false } },
      yaxis: { lines: { show: true } },
    },
    responsive: [
      {
        breakpoint: 900,
        options: { legend: { show: false } },
      },
    ],
  };
});

// Force ApexCharts to re-render options when mode/overlays toggle
const chartKey = computed(
  () =>
    `${state.chartMode.value}-${state.showCumulative.value}-${state.showCitations.value}-${yearFrom.value}-${yearTo.value}-${state.selectedJournals.value.length}`
);

// ── Secondary chart: top-10 journals horizontal bar ─────────────
const journalTotals = computed(() => {
  const totals = new Map<string, number>();
  for (const inner of journalByYearIndex.value.values()) {
    for (const [journal, { count }] of inner) {
      totals.set(journal, (totals.get(journal) ?? 0) + count);
    }
  }
  return [...totals.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
});

const journalChartSeries = computed(() => [
  {
    name: 'Articles',
    data: journalTotals.value.map(([, count]) => count),
  },
]);

const journalChartOptions = computed<ApexOptions>(() => ({
  chart: {
    type: 'bar',
    height: 200,
    toolbar: { show: false },
    animations: { enabled: false },
    fontFamily: 'inherit',
    background: 'transparent',
  },
  plotOptions: {
    bar: {
      horizontal: true,
      borderRadius: 3,
      distributed: true,
    },
  },
  colors: OKABE_ITO_10,
  series: journalChartSeries.value,
  dataLabels: {
    enabled: true,
    textAnchor: 'start',
    offsetX: 8,
    style: { colors: ['#fff'], fontSize: '11px', fontWeight: 600 },
    formatter: (_val: string, opts) => {
      const seriesData = (opts?.w?.config?.series as { data: number[] }[]) ?? [];
      const dataArr = seriesData[0]?.data ?? [];
      const total = dataArr.reduce((s, v) => s + v, 0);
      const v = dataArr[opts?.dataPointIndex ?? 0] ?? 0;
      return total > 0 ? `${((v / total) * 100).toFixed(0)}%` : '';
    },
  },
  xaxis: {
    categories: journalTotals.value.map(([name]) => name),
    labels: { style: { colors: '#475569', fontSize: '11px' } },
    axisBorder: { show: false },
    axisTicks: { show: false },
  },
  yaxis: {
    labels: {
      style: { colors: '#334155', fontSize: '11px' },
      maxWidth: 220,
    },
  },
  legend: { show: false },
  tooltip: { theme: 'light' },
  grid: {
    borderColor: '#f1f5f9',
    strokeDashArray: 3,
    xaxis: { lines: { show: true } },
    yaxis: { lines: { show: false } },
  },
}));

const journalChartKey = computed(() => `journals-${yearFrom.value}-${yearTo.value}`);

// After chart renders, inject `title` attributes on legend items so hovering
// shows the full journal name as a native browser tooltip.
watch(chartKey, () => {
  void nextTick(() => {
    const legendItems = document.querySelectorAll('.chart-primary .apexcharts-legend-series');
    const allSeries = (chartOptions.value.series ?? []) as { name?: string }[];
    legendItems.forEach((el, idx) => {
      const seriesName = allSeries[idx]?.name ?? '';
      const fullName = journalFullNameMap.value.get(seriesName);
      if (fullName && fullName !== seriesName) {
        el.setAttribute('title', fullName);
      }
    });
  });
});

// ── Year Detail Panel data ─────────────────────────────────────
interface JournalRow {
  journal: string;
  count: number;
  percent: number;
  journalIndexId: string | null;
}

const yearPanelData = computed(() => {
  if (state.selectedYear.value === null) return null;
  const y = state.selectedYear.value;
  const yc = kpis.value.pubsByYear.find((p) => p.year === y);
  if (!yc) return null;
  const inner = journalByYearIndex.value.get(y);
  const rows: JournalRow[] = [];
  if (inner) {
    for (const [journal, { count, journalIndexId }] of inner) {
      rows.push({
        journal,
        count,
        percent: yc.count > 0 ? (count / yc.count) * 100 : 0,
        journalIndexId,
      });
    }
    rows.sort((a, b) => b.count - a.count);
  }
  const cit = kpis.value.citationsByYear.find((c) => c.year === y);
  const idx = kpis.value.pubsByYear.findIndex((p) => p.year === y);
  const prev = idx > 0 ? kpis.value.pubsByYear[idx - 1] : null;
  const growth = prev && prev.count > 0 ? ((yc.count - prev.count) / prev.count) * 100 : null;
  return { yc, rows, citationCount: cit?.count ?? null, growth };
});

// ── Bar / data-point click → year detail ───────────────────────
// Defensive: ApexCharts emits dataPointIndex = -1 on empty-space clicks and
// can exceed bounds on edge hovers. Guard every access.
function handleDataPointClick(
  _event: unknown,
  _config: unknown,
  opts?: { dataPointIndex?: number }
): void {
  const idx = opts?.dataPointIndex;
  if (idx === undefined || idx === null || idx < 0 || idx >= filteredPubs.value.length) return;
  const entry = filteredPubs.value[idx];
  if (!entry) return;
  state.selectedYear.value = state.selectedYear.value === entry.year ? null : entry.year;
}

// ── Legend click → open Journal Info Card ──────────────────────
// When user clicks a legend item in stacked mode, open the journal info card
// for that journal (in addition to the default toggle behavior).
function handleLegendClick(_chart: unknown, seriesIndex?: number): void {
  if (seriesIndex === undefined || seriesIndex < 0) return;
  // Get the series name (normalized) and look up the full journal name
  const allSeries = (chartOptions.value.series ?? []) as { name?: string }[];
  const clickedSeries = allSeries[seriesIndex];
  if (!clickedSeries || !clickedSeries.name || clickedSeries.name === 'Other') return;
  const fullName = journalFullNameMap.value.get(clickedSeries.name);
  if (!fullName) return;
  // Find the journalIndexId from the journal data
  for (const inner of journalByYearIndex.value.values()) {
    const entry = inner.get(fullName);
    if (entry) {
      selectedJournalKey.value = fullName;
      selectedJournalIndexId.value = entry.journalIndexId;
      return;
    }
  }
}

// ── Journal Info Card selection ────────────────────────────────
function selectJournal(row: JournalRow): void {
  selectedJournalKey.value = row.journal;
  selectedJournalIndexId.value = row.journalIndexId;
}

function closeJournalCard(): void {
  selectedJournalKey.value = null;
  selectedJournalIndexId.value = null;
}

// ── Deep links ─────────────────────────────────────────────────
function viewYearArticles(year: number): void {
  void router.push({
    name: 'articles',
    query: { yearFrom: year, yearTo: year, filterCollapsed: '1', from: 'timeline' },
  });
}

// ── Export via ApexCharts dataURI + Tauri save dialog ──────────
async function handleExportPng(): Promise<void> {
  try {
    const chart = chartRef.value;
    if (!chart) return;
    const result = await (
      chart as unknown as { dataURI: () => Promise<{ imgURI: string }> }
    ).dataURI();
    const { save } = await import('@tauri-apps/plugin-dialog');
    const filePath = await save({
      defaultPath: 'publication-timeline.png',
      filters: [{ name: 'PNG Image', extensions: ['png'] }],
    });
    if (!filePath) return;
    const base64 = result.imgURI.split(',')[1] ?? '';
    await tauriCommand('write_base64_to_file', { path: filePath, data: base64 });
  } catch (e) {
    console.error('PNG export failed', e);
  }
}

async function handleExportSvg(): Promise<void> {
  try {
    // ApexCharts dataURI({ fileExt: 'svg' }) has reliability issues in
    // vue3-apexcharts (returns incomplete data). Instead, serialize the
    // chart's SVG DOM element directly via XMLSerializer — reliable and
    // produces the full SVG markup.
    const chartEl = document.querySelector('.chart-primary svg');
    if (!chartEl) {
      console.error('SVG export failed: chart SVG element not found');
      return;
    }
    const clone = chartEl.cloneNode(true) as SVGElement;
    clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
    clone.setAttribute('xmlns:xlink', 'http://www.w3.org/1999/xlink');
    const svgString = new XMLSerializer().serializeToString(clone);

    const { save } = await import('@tauri-apps/plugin-dialog');
    const filePath = await save({
      defaultPath: 'publication-timeline.svg',
      filters: [{ name: 'SVG', extensions: ['svg'] }],
    });
    if (!filePath) return;
    await tauriCommand('write_text_to_file', { path: filePath, content: svgString });
  } catch (e) {
    console.error('SVG export failed', e);
  }
}

// ── Journal checkbox toggle ────────────────────────────────────
function toggleJournal(journal: string): void {
  const idx = state.selectedJournals.value.indexOf(journal);
  if (idx >= 0) {
    state.selectedJournals.value.splice(idx, 1);
  } else {
    state.selectedJournals.value.push(journal);
  }
}

function resetFilters(): void {
  state.reset(rangeMin.value, rangeMax.value);
  closeJournalCard();
}

// Capture unhandled promise rejections from ApexCharts internal SVG.js
// operations (Element not found during chart destroy/update cycles).
// These are non-fatal but noisy — log them once for diagnostics.
onMounted(async () => {
  const rejectionHandler = (e: PromiseRejectionEvent) => {
    const reason = e.reason instanceof Error ? e.reason.message : String(e.reason);
    if (reason.includes('Element not found') || reason.includes('elDefs')) {
      // Non-fatal: ApexCharts SVG.js internal cleanup race during chart remount.
      // Swallow to prevent console noise.
      e.preventDefault();
    }
  };
  window.addEventListener('unhandledrejection', rejectionHandler);
  // Store for cleanup
  (window as unknown as Record<string, unknown>).__timelineRejectionHandler = rejectionHandler;

  await fetchKpis();
  // Initialize range bounds now that data is loaded. Doing this here (not in
  // an immediate watcher) ensures chartKey is stable before VueApexCharts mounts.
  if (kpis.value.includedCount > 0) {
    state.setRange(rangeMin.value, rangeMax.value);
  }
});

onUnmounted(() => {
  const handler = (window as unknown as Record<string, unknown>).__timelineRejectionHandler as
    | ((e: PromiseRejectionEvent) => void)
    | undefined;
  if (handler) {
    window.removeEventListener('unhandledrejection', handler);
  }
});
</script>

<template>
  <div class="timeline-layout">
    <!-- Empty state -->
    <div v-if="kpis.includedCount === 0 && !loading" class="timeline-empty">
      <span class="material-symbols-outlined timeline-empty__icon">inbox</span>
      <p class="timeline-empty__text">
        No included articles yet. Include articles to see the publication timeline.
      </p>
    </div>

    <template v-else>
      <!-- ── Sidebar ───────────────────────────────────────── -->
      <div class="sidebar-wrapper" :class="state.sidebarCollapsed.value ? 'w-0' : 'w-64'">
        <aside class="sidebar" :class="{ 'sidebar--collapsed': state.sidebarCollapsed.value }">
          <div class="sidebar__scroll">
            <!-- Chart Options -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Chart Options</h4>
              <div class="seg-toggle" role="tablist" aria-label="Chart mode">
                <button
                  v-for="mode in ['bars', 'line', 'stacked'] as const"
                  :key="mode"
                  role="tab"
                  :aria-selected="state.chartMode.value === mode"
                  :class="[
                    'seg-toggle__btn',
                    { 'seg-toggle__btn--active': state.chartMode.value === mode },
                  ]"
                  @click="state.chartMode.value = mode"
                >
                  {{ mode === 'bars' ? 'Bars' : mode === 'line' ? 'Line' : 'Stacked' }}
                </button>
              </div>
              <label v-if="state.chartMode.value !== 'stacked'" class="sidebar__check">
                <input v-model="state.showCumulative.value" type="checkbox" />
                <span>Cumulative overlay</span>
              </label>
              <label v-if="state.chartMode.value !== 'stacked'" class="sidebar__check">
                <input v-model="state.showCitations.value" type="checkbox" />
                <span>Citations overlay</span>
              </label>
            </section>

            <!-- Dual-handle year range slider -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Year Range</h4>
              <div class="dual-range-block">
                <div class="dual-range-header">
                  <span class="dual-range-value">{{ yearFrom }} – {{ yearTo }}</span>
                  <button
                    v-if="yearFrom !== rangeMin || yearTo !== rangeMax"
                    class="dual-range-reset"
                    @click="
                      yearFrom = rangeMin;
                      yearTo = rangeMax;
                    "
                  >
                    Reset
                  </button>
                </div>
                <div class="dual-range-track">
                  <div class="dual-range-bar-bg"></div>
                  <div
                    class="dual-range-bar-active"
                    :style="{
                      left: `${((yearFrom - rangeMin) / Math.max(1, rangeMax - rangeMin)) * 100}%`,
                      right: `${((rangeMax - yearTo) / Math.max(1, rangeMax - rangeMin)) * 100}%`,
                    }"
                  ></div>
                  <input
                    v-model.number="state.yearFrom.value"
                    type="range"
                    :min="rangeMin"
                    :max="rangeMax"
                    step="1"
                    class="dual-range-input"
                    aria-label="Year from"
                  />
                  <input
                    v-model.number="state.yearTo.value"
                    type="range"
                    :min="rangeMin"
                    :max="rangeMax"
                    step="1"
                    class="dual-range-input"
                    aria-label="Year to"
                  />
                </div>
                <div class="dual-range-endpoints">
                  <span>{{ rangeMin }}</span>
                  <span>{{ rangeMax }}</span>
                </div>
              </div>
            </section>

            <!-- Journals filter (stacked mode only) -->
            <section v-if="state.chartMode.value === 'stacked'" class="sidebar__section">
              <h4 class="sidebar__label">Journals (Top 10)</h4>
              <ul class="journal-list">
                <li v-for="j in topJournals" :key="j" class="journal-list__item">
                  <label class="journal-list__check" :title="j">
                    <input
                      type="checkbox"
                      :checked="state.selectedJournals.value.includes(j)"
                      @change="toggleJournal(j)"
                    />
                    <span class="journal-list__name">{{
                      j.length > 28 ? j.slice(0, 28) + '…' : j
                    }}</span>
                  </label>
                </li>
              </ul>
            </section>
            <p v-else class="sidebar__hint">Journal filter applies to Stacked mode only.</p>

            <!-- Stats -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Stats</h4>
              <p class="sidebar__stat-line">Showing {{ yearFrom }} – {{ yearTo }}</p>
              <p class="sidebar__stat-line">{{ totalInRange }} articles in range</p>
            </section>

            <!-- Export -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Export</h4>
              <button class="sidebar__export" @click="handleExportPng">
                <span class="material-symbols-outlined">image</span>
                Export PNG
              </button>
              <button class="sidebar__export" @click="handleExportSvg">
                <span class="material-symbols-outlined">share</span>
                Export SVG
              </button>
            </section>

            <button class="sidebar__reset" @click="resetFilters">Reset All Filters</button>
          </div>
        </aside>
        <button
          class="drawer-handle"
          :title="state.sidebarCollapsed.value ? 'Show sidebar' : 'Hide sidebar'"
          :style="{ left: state.sidebarCollapsed.value ? '0px' : 'calc(100% - 16px)' }"
          @click="state.sidebarCollapsed.value = !state.sidebarCollapsed.value"
        >
          <span class="drawer-handle-grip"></span>
        </button>
      </div>

      <!-- ── Main canvas ────────────────────────────────────── -->
      <main class="timeline-main">
        <!-- KPI strip -->
        <section class="kpi-strip">
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ totalInRange.toLocaleString() }}</span>
            <span class="kpi-mini__label">Total Pubs</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ peakEntry?.year ?? '—' }}</span>
            <span class="kpi-mini__label">Peak Year</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ peakEntry?.count ?? '—' }}</span>
            <span class="kpi-mini__label">Peak Count</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ avgPerYear }}</span>
            <span class="kpi-mini__label">Avg / Year</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">
              {{
                kpis.avgGrowthRate !== null
                  ? `${kpis.avgGrowthRate >= 0 ? '+' : ''}${kpis.avgGrowthRate.toFixed(1)}%`
                  : '—'
              }}
            </span>
            <span class="kpi-mini__label">Avg Growth</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ kpis.totalCitations.toLocaleString() }}</span>
            <span class="kpi-mini__label">Total Citations</span>
          </div>
        </section>

        <!-- Charts: primary time-series + secondary journal breakdown -->
        <div class="charts-container">
          <div v-if="loading" class="chart-loading">
            <span class="material-symbols-outlined chart-spin">progress_activity</span>
          </div>
          <template v-else-if="filteredPubs.length > 0">
            <!-- Primary: time-series -->
            <div class="chart-primary">
              <VueApexCharts
                :key="chartKey"
                ref="chartRef"
                :options="chartOptions"
                :series="chartOptions.series"
                :height="primaryChartHeight"
                class="chart-apex"
              />
            </div>
            <!-- Secondary: top-10 journals horizontal bar (hidden when viewport height is too short) -->
            <div v-if="journalTotals.length > 0 && showSecondaryChart" class="chart-secondary">
              <h6 class="chart-secondary__title">Top Journals in Range</h6>
              <VueApexCharts
                :key="journalChartKey"
                :options="journalChartOptions"
                :series="journalChartOptions.series"
                :height="secondaryChartHeight"
                class="chart-apex"
              />
            </div>
          </template>
          <div v-else class="chart-empty">
            <span class="material-symbols-outlined">filter_alt_off</span>
            <p>No articles in the selected range.</p>
          </div>
        </div>
      </main>

      <!-- ── Year Detail Panel ──────────────────────────────── -->
      <Transition name="detail-slide">
        <aside v-if="state.selectedYear.value !== null && yearPanelData" class="year-panel">
          <header class="year-panel__header">
            <div class="year-panel__badge">{{ state.selectedYear.value }}</div>
            <button
              class="year-panel__close"
              title="Close"
              @click="state.selectedYear.value = null"
            >
              <span class="material-symbols-outlined">close</span>
            </button>
          </header>

          <div class="year-panel__body">
            <div class="year-panel__count">
              <span class="year-panel__count-value">{{ yearPanelData.yc.count }}</span>
              <span class="year-panel__count-label">publications</span>
            </div>

            <div class="year-panel__meta">
              <div v-if="yearPanelData.citationCount !== null" class="year-panel__meta-item">
                <span class="year-panel__meta-label">Citations</span>
                <span class="year-panel__meta-value">{{ yearPanelData.citationCount }}</span>
              </div>
              <div v-if="yearPanelData.growth !== null" class="year-panel__meta-item">
                <span class="year-panel__meta-label">Growth vs prev.</span>
                <span
                  class="year-panel__pill"
                  :class="
                    yearPanelData.growth >= 0 ? 'year-panel__pill--up' : 'year-panel__pill--down'
                  "
                >
                  {{ yearPanelData.growth >= 0 ? '+' : '' }}{{ yearPanelData.growth.toFixed(1) }}%
                </span>
              </div>
            </div>

            <div v-if="yearPanelData.rows.length > 0" class="year-panel__journals">
              <h5 class="year-panel__subhead">Journals</h5>
              <ul class="year-panel__journal-list">
                <li v-for="row in yearPanelData.rows" :key="row.journal">
                  <button
                    class="year-panel__journal-row"
                    :class="{
                      'year-panel__journal-row--active': selectedJournalKey === row.journal,
                    }"
                    :title="row.journal"
                    @click="selectJournal(row)"
                  >
                    <span class="year-panel__journal-name">{{ row.journal || '(blank)' }}</span>
                    <span class="year-panel__journal-count">{{ row.count }}</span>
                    <span class="year-panel__journal-pct">{{ row.percent.toFixed(0) }}%</span>
                  </button>
                </li>
              </ul>
            </div>

            <button
              class="year-panel__view-btn"
              @click="viewYearArticles(state.selectedYear.value)"
            >
              <span class="material-symbols-outlined">article</span>
              View articles from {{ state.selectedYear.value }}
            </button>
          </div>
        </aside>
      </Transition>

      <!-- ── Journal Info Card ──────────────────────────────── -->
      <JournalInfoCard
        :journal-index-id="selectedJournalIndexId"
        :is-raw="selectedJournalIndexId === null && selectedJournalKey !== null"
        @close="closeJournalCard"
      />
    </template>
  </div>
</template>

<style scoped>
.timeline-layout {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}

/* ── Sidebar ─────────────────────────────────────────────────── */
.sidebar-wrapper {
  position: relative;
  flex-shrink: 0;
  transition: width 0.3s;
}

.sidebar {
  height: 100%;
  width: 16rem;
  overflow-y: auto;
  border-right: 1px solid #f1f5f9;
  background: #fafbfc;
  transition:
    opacity 0.3s,
    width 0.3s,
    padding 0.3s;
}

.sidebar--collapsed {
  width: 0;
  padding: 0;
  overflow: hidden;
  opacity: 0;
}

.sidebar__scroll {
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.sidebar__section {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.sidebar__label {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #64748b;
  margin: 0;
}

.sidebar__hint {
  font-size: 0.6875rem;
  color: #94a3b8;
  font-style: italic;
  margin: 0;
}

.seg-toggle {
  display: flex;
  border: 1px solid #e2e8f0;
  border-radius: 0.375rem;
  overflow: hidden;
}

.seg-toggle__btn {
  flex: 1;
  padding: 0.375rem 0.5rem;
  border: none;
  background: #ffffff;
  font-size: 0.75rem;
  font-weight: 500;
  color: #64748b;
  cursor: pointer;
  transition:
    background 0.15s,
    color 0.15s;
}

.seg-toggle__btn:not(:last-child) {
  border-right: 1px solid #e2e8f0;
}

.seg-toggle__btn--active {
  background: var(--color-primary, #4f46e5);
  color: #ffffff;
}

.sidebar__check {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 0.75rem;
  color: #334155;
  cursor: pointer;
}

.sidebar__check input {
  margin: 0;
}

/* ── Dual-handle range slider ────────────────────────────────── */
.dual-range-block {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.dual-range-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.dual-range-value {
  font-size: 0.75rem;
  font-weight: 600;
  color: #1e293b;
  font-variant-numeric: tabular-nums;
}

.dual-range-reset {
  background: none;
  border: none;
  font-size: 0.625rem;
  color: #4f46e5;
  cursor: pointer;
  padding: 0;
}

.dual-range-track {
  position: relative;
  height: 1.5rem;
  display: flex;
  align-items: center;
}

.dual-range-bar-bg {
  position: absolute;
  inset: 0 0 0 0;
  height: 0.375rem;
  background: #e2e8f0;
  border-radius: 9999px;
  top: 50%;
  transform: translateY(-50%);
}

.dual-range-bar-active {
  position: absolute;
  height: 0.375rem;
  background: #6366f1;
  border-radius: 9999px;
  top: 50%;
  transform: translateY(-50%);
  pointer-events: none;
}

.dual-range-input {
  position: absolute;
  inset: 0;
  width: 100%;
  pointer-events: none;
  background: transparent;
  appearance: none;
  -webkit-appearance: none;
}

.dual-range-input::-webkit-slider-thumb {
  pointer-events: auto;
  appearance: none;
  -webkit-appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  border: 2px solid #6366f1;
  box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  cursor: pointer;
  position: relative;
  z-index: 2;
}

.dual-range-input::-moz-range-thumb {
  pointer-events: auto;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  border: 2px solid #6366f1;
  box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  cursor: pointer;
  position: relative;
  z-index: 2;
}

.dual-range-input::-webkit-slider-runnable-track {
  background: transparent;
}

.dual-range-input::-moz-range-track {
  background: transparent;
}

.dual-range-endpoints {
  display: flex;
  justify-content: space-between;
  font-size: 0.625rem;
  color: #94a3b8;
}

/* ── Journal list ────────────────────────────────────────────── */
.journal-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  max-height: 16rem;
  overflow-y: auto;
}

.journal-list__item {
  margin: 0;
}

.journal-list__check {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 0.75rem;
  color: #334155;
  cursor: pointer;
  padding: 0.125rem 0;
}

.journal-list__check input {
  margin: 0;
  flex-shrink: 0;
}

.journal-list__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar__reset,
.sidebar__export {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  padding: 0.375rem 0.5rem;
  border: 1px solid #e2e8f0;
  border-radius: 0.375rem;
  background: #ffffff;
  font-size: 0.75rem;
  font-weight: 500;
  color: #475569;
  cursor: pointer;
  font-family: inherit;
  transition:
    border-color 0.15s,
    color 0.15s;
}

.sidebar__reset:hover,
.sidebar__export:hover {
  border-color: #818cf8;
  color: #4f46e5;
}

.sidebar__reset .material-symbols-outlined,
.sidebar__export .material-symbols-outlined {
  font-size: 0.9375rem;
}

.sidebar__stat-line {
  margin: 0;
  font-size: 0.75rem;
  color: #475569;
}

/* ── Drawer handle ───────────────────────────────────────────── */
.drawer-handle {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 30;
  width: 14px;
  height: 72px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #f8fafc;
  border: 1px solid #e2e8f0;
  border-left: none;
  border-radius: 0 8px 8px 0;
  box-shadow: 2px 0 4px rgba(0, 0, 0, 0.06);
  cursor: pointer;
  transition:
    left 0.3s,
    background-color 0.15s,
    border-color 0.15s,
    width 0.15s;
}

.drawer-handle:hover {
  background: #eef2ff;
  border-color: #a5b4fc;
  width: 16px;
}

.drawer-handle-grip {
  display: flex;
  flex-direction: column;
  gap: 3px;
  align-items: center;
}

.drawer-handle-grip::before,
.drawer-handle-grip::after,
.drawer-handle-grip {
  content: '';
  display: block;
  width: 4px;
  height: 2px;
  border-radius: 1px;
  background: #94a3b8;
  transition: background-color 0.15s;
}

.drawer-handle:hover .drawer-handle-grip::before,
.drawer-handle:hover .drawer-handle-grip::after,
.drawer-handle:hover .drawer-handle-grip {
  background: #6366f1;
}

/* ── Main ────────────────────────────────────────────────────── */
.timeline-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* ── Rematch banner ──────────────────────────────────────────── */
.rematch-banner {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 1rem;
  background: #fffbeb;
  border-bottom: 1px solid #fde68a;
  font-size: 0.75rem;
  color: #92400e;
}

.rematch-banner__icon {
  font-size: 1rem;
}

.rematch-banner__btn {
  background: none;
  border: none;
  color: #4f46e5;
  font-weight: 600;
  cursor: pointer;
  font-size: inherit;
  font-family: inherit;
  text-decoration: underline;
  padding: 0;
}

.rematch-banner__btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

/* ── KPI strip ───────────────────────────────────────────────── */
.kpi-strip {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 0.5rem;
  padding: 0.75rem 1rem;
  background: #ffffff;
  border-bottom: 1px solid #f1f5f9;
}

@media (max-width: 900px) {
  .kpi-strip {
    grid-template-columns: repeat(3, 1fr);
  }
}

.kpi-mini {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.kpi-mini__value {
  font-size: 1.125rem;
  font-weight: 800;
  color: #1e293b;
  line-height: 1.1;
}

.kpi-mini__label {
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
  font-weight: 600;
}

/* ── Charts layout ───────────────────────────────────────────── */
.charts-container {
  flex: 1;
  position: relative;
  overflow: hidden;
  padding: 0.5rem 1rem;
  background: #ffffff;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.chart-primary {
  flex: 1 1 55%;
  min-height: 0;
  display: flex;
}

.chart-secondary {
  flex: 0 0 auto;
  height: 16rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  border-top: 1px solid #f1f5f9;
  padding-top: 0.5rem;
}

.chart-secondary__title {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #64748b;
  margin: 0;
}

.chart-apex {
  width: 100%;
}

.chart-loading,
.chart-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  height: 100%;
  color: #94a3b8;
  font-size: 0.875rem;
}

.chart-spin {
  animation: chart-spin-anim 1s linear infinite;
  font-size: 2rem;
  color: var(--color-primary, #4f46e5);
}

@keyframes chart-spin-anim {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.chart-empty .material-symbols-outlined {
  font-size: 2rem;
}

/* ── Empty state ─────────────────────────────────────────────── */
.timeline-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  color: #94a3b8;
}

.timeline-empty__icon {
  font-size: 3rem;
}

.timeline-empty__text {
  font-size: 0.875rem;
  margin: 0;
}

/* ── Year Detail Panel ───────────────────────────────────────── */
.year-panel {
  width: 18rem;
  flex-shrink: 0;
  background: #ffffff;
  border-left: 1px solid #e2e8f0;
  box-shadow: -8px 0 24px rgba(0, 0, 0, 0.08);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.year-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid #f1f5f9;
}

.year-panel__badge {
  font-size: 1.5rem;
  font-weight: 800;
  color: #d97706;
  line-height: 1;
}

.year-panel__close {
  background: none;
  border: none;
  cursor: pointer;
  padding: 0.25rem;
  border-radius: 0.25rem;
  color: #94a3b8;
  display: flex;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.year-panel__close:hover {
  background: #f1f5f9;
  color: #475569;
}

.year-panel__close .material-symbols-outlined {
  font-size: 1.125rem;
}

.year-panel__body {
  flex: 1;
  overflow-y: auto;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.year-panel__count {
  display: flex;
  align-items: baseline;
  gap: 0.375rem;
}

.year-panel__count-value {
  font-size: 1.75rem;
  font-weight: 800;
  color: #1e293b;
  line-height: 1;
}

.year-panel__count-label {
  font-size: 0.75rem;
  color: #64748b;
}

.year-panel__meta {
  display: flex;
  gap: 1rem;
}

.year-panel__meta-item {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.year-panel__meta-label {
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
  font-weight: 600;
}

.year-panel__meta-value {
  font-size: 0.875rem;
  font-weight: 600;
  color: #334155;
}

.year-panel__pill {
  display: inline-block;
  font-size: 0.75rem;
  font-weight: 600;
  padding: 0.125rem 0.5rem;
  border-radius: 0.25rem;
}

.year-panel__pill--up {
  background: #dcfce7;
  color: #166534;
}

.year-panel__pill--down {
  background: #fee2e2;
  color: #991b1b;
}

.year-panel__journals {
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.year-panel__subhead {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: #64748b;
  margin: 0;
}

.year-panel__journal-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  max-height: 16rem;
  overflow-y: auto;
}

.year-panel__journal-row {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: 0.5rem;
  align-items: center;
  width: 100%;
  padding: 0.25rem 0.375rem;
  border: none;
  background: transparent;
  border-radius: 0.25rem;
  font-size: 0.75rem;
  color: #334155;
  cursor: pointer;
  text-align: left;
  font-family: inherit;
  transition: background-color 0.15s;
}

.year-panel__journal-row:hover {
  background: #f8fafc;
}

.year-panel__journal-row--active {
  background: #eef2ff;
}

.year-panel__journal-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.year-panel__journal-count {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.year-panel__journal-pct {
  color: #94a3b8;
  font-size: 0.6875rem;
  font-variant-numeric: tabular-nums;
  width: 2rem;
  text-align: right;
}

.year-panel__view-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  background: var(--color-primary, #4f46e5);
  color: #ffffff;
  border: none;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  margin-top: auto;
  transition: opacity 0.15s;
}

.year-panel__view-btn:hover {
  opacity: 0.9;
}

.year-panel__view-btn .material-symbols-outlined {
  font-size: 1.125rem;
}

/* ── Detail slide transition ─────────────────────────────────── */
.detail-slide-enter-active,
.detail-slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}
.detail-slide-enter-from,
.detail-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
