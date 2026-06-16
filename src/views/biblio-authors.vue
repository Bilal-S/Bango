<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { openUrl } from '@tauri-apps/plugin-opener';
import { save } from '@tauri-apps/plugin-dialog';
import VueApexCharts from 'vue3-apexcharts';
import type { ApexOptions } from 'apexcharts';
import { useAuthorRankings } from '@/composables/use-author-rankings';
import type { AuthorRank } from '@/composables/use-author-rankings';
import { useAuthorDetail } from '@/composables/use-author-detail';
import { tauriCommand } from '@/composables/use-tauri-command';

const router = useRouter();
const { rankings, kpis, loading, error } = useAuthorRankings();
const { detail, loading: detailLoading, getAuthorDetail, clear } = useAuthorDetail();

// ── Sidebar state ────────────────────────────────────────────────
const sidebarCollapsed = ref(false);
const minPapers = ref(1);
const topN = ref(0); // 0 = All
const yearFrom = ref<number | null>(null);
const yearTo = ref<number | null>(null);

// ── Sortable table columns ───────────────────────────────────────
interface SortColumn {
  key: keyof AuthorRank | 'index';
  label: string;
  numeric: boolean;
}

const columns: SortColumn[] = [
  { key: 'index', label: '#', numeric: true },
  { key: 'displayName', label: 'Author', numeric: false },
  { key: 'articleCount', label: 'Papers', numeric: true },
  { key: 'totalCitations', label: 'Citations', numeric: true },
  { key: 'estimatedHIndex', label: 'h', numeric: true },
  { key: 'i10Index', label: 'i10', numeric: true },
  { key: 'gIndex', label: 'g', numeric: true },
  { key: 'firstAuthorCount', label: 'First', numeric: true },
  { key: 'lastAuthorCount', label: 'Last', numeric: true },
  { key: 'avgCitationsPerPaper', label: 'Avg/Paper', numeric: true },
];

// ── Sort state + logic ───────────────────────────────────────────
const sortColumn = ref<keyof AuthorRank>('estimatedHIndex');
const sortDirection = ref<'asc' | 'desc'>('desc');

function toggleSort(col: SortColumn): void {
  if (col.key === 'index') return;
  if (sortColumn.value === col.key) {
    sortDirection.value = sortDirection.value === 'asc' ? 'desc' : 'asc';
  } else {
    sortColumn.value = col.key;
    sortDirection.value = 'desc';
  }
}

function getSortIcon(col: SortColumn): string {
  if (col.key === 'index') return '';
  if (sortColumn.value !== col.key) return 'arrow_upward';
  return sortDirection.value === 'asc' ? 'arrow_upward' : 'arrow_downward';
}

function isSortActive(col: SortColumn): boolean {
  return col.key !== 'index' && sortColumn.value === col.key;
}

// ── Year range bounds ────────────────────────────────────────────
const yearMin = computed(() => kpis.value?.yearFrom ?? 2000);
const yearMax = computed(() => kpis.value?.yearTo ?? new Date().getFullYear());

// Initialize year range once KPIs load
watch(
  () => kpis.value,
  (k) => {
    if (k && yearFrom.value === null && k.yearFrom !== null) {
      yearFrom.value = k.yearFrom;
      yearTo.value = k.yearTo;
    }
  },
  { immediate: true }
);

// ── Filtered + sorted rankings ───────────────────────────────────
const sortedRankings = computed(() => {
  const col = sortColumn.value;
  const dir = sortDirection.value === 'asc' ? 1 : -1;
  return [...rankings.value].sort((a, b) => {
    const av = a[col];
    const bv = b[col];
    if (typeof av === 'number' && typeof bv === 'number') return (av - bv) * dir;
    return String(av).localeCompare(String(bv)) * dir;
  });
});

const filteredRankings = computed(() => {
  let list = sortedRankings.value.filter((a) => a.articleCount >= minPapers.value);
  // Year range filter (on avgYear)
  if (yearFrom.value !== null && yearTo.value !== null) {
    list = list.filter((a) => {
      if (a.avgYear === null) return false;
      return a.avgYear >= yearFrom.value! && a.avgYear <= yearTo.value!;
    });
  }
  // Top-N
  if (topN.value > 0) list = list.slice(0, topN.value);
  return list;
});

// ── Selected author for detail panel ─────────────────────────────
const selectedAuthorId = ref<string | null>(null);

watch(selectedAuthorId, (id) => {
  if (id) void getAuthorDetail(id);
  else clear();
});

function selectAuthor(author: AuthorRank): void {
  selectedAuthorId.value = selectedAuthorId.value === author.id ? null : author.id;
}

function closeDetail(): void {
  selectedAuthorId.value = null;
}

// ── Scatter chart (Productivity vs Impact) ───────────────────────
const scatterRef = ref<InstanceType<typeof VueApexCharts> | null>(null);

const scatterSeries = computed(() => {
  return [
    {
      name: 'Authors',
      data: filteredRankings.value.map((a) => ({
        x: a.articleCount,
        y: a.totalCitations,
        z: Math.max(1, a.estimatedHIndex),
        author: a,
      })),
    },
  ];
});

function avgYearColor(year: number | null): string {
  if (year === null) return '#94a3b8';
  const min = yearMin.value;
  const max = yearMax.value;
  const range = Math.max(1, max - min);
  const t = Math.min(1, Math.max(0, (year - min) / range));
  // Blue (senior) → Red (early-career)
  const r = Math.round(59 + (239 - 59) * t);
  const g = Math.round(130 + (68 - 130) * t);
  const b = Math.round(246 + (68 - 246) * t);
  return `rgb(${r},${g},${b})`;
}

const scatterOptions = computed<ApexOptions>(() => ({
  chart: {
    type: 'bubble',
    height: 320,
    toolbar: { show: false },
    animations: { enabled: false },
    fontFamily: 'inherit',
    background: 'transparent',
    zoom: { enabled: true },
    events: {
      dataPointSelection: (_e: unknown, _c: unknown, opts?: { dataPointIndex?: number }) => {
        const idx = opts?.dataPointIndex;
        if (idx === undefined || idx < 0 || idx >= filteredRankings.value.length) return;
        selectAuthor(filteredRankings.value[idx]!);
      },
    },
  },
  dataLabels: { enabled: false },
  fill: { opacity: 0.7 },
  colors: filteredRankings.value.map((a) => avgYearColor(a.avgYear)),
  xaxis: {
    title: { text: 'Papers', style: { color: '#475569', fontSize: '12px' } },
    labels: { style: { colors: '#94a3b8', fontSize: '10px' } },
    tickAmount: 6,
  },
  yaxis: {
    title: { text: 'Citations', style: { color: '#475569', fontSize: '12px' } },
    labels: { style: { colors: '#94a3b8', fontSize: '10px' } },
  },
  tooltip: {
    theme: 'light',
    custom: ({ dataPointIndex }: { dataPointIndex?: number }) => {
      const idx = dataPointIndex;
      if (idx === undefined || idx < 0 || idx >= filteredRankings.value.length) return '';
      const a = filteredRankings.value[idx]!;
      return `<div class="apexcharts-tooltip-title">${a.displayName}</div>
        <div class="apexcharts-tooltip-series-group">
          Papers: <strong>${a.articleCount}</strong> · Citations: <strong>${a.totalCitations}</strong><br/>
          h: <strong>${a.estimatedHIndex}</strong> · i10: <strong>${a.i10Index}</strong><br/>
          ${a.primaryInstitution ?? ''}
        </div>`;
    },
  },
  grid: { borderColor: '#f1f5f9', strokeDashArray: 3 },
  plotOptions: {
    bubble: { minBubbleRadius: 4, maxBubbleRadius: 20 },
  },
  legend: { show: false },
}));

const scatterKey = computed(
  () =>
    `${sortColumn.value}-${sortDirection.value}-${minPapers.value}-${yearFrom.value}-${yearTo.value}-${topN.value}`
);

// ── Draggable splitter between scatter and table ─────────────────
const scatterHeight = ref(320);
const isDragging = ref(false);

function onSplitterMouseDown(e: MouseEvent): void {
  e.preventDefault();
  isDragging.value = true;
  const startY = e.clientY;
  const startHeight = scatterHeight.value;
  const container = (e.currentTarget as HTMLElement).closest('.authors-main');
  const maxHeight = (container?.clientHeight ?? 600) - 120;

  function onMove(ev: MouseEvent): void {
    const delta = ev.clientY - startY;
    scatterHeight.value = Math.max(60, Math.min(maxHeight, startHeight + delta));
  }
  function onUp(): void {
    isDragging.value = false;
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
  }
  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
}

function onSplitterDoubleClick(): void {
  scatterHeight.value = scatterHeight.value <= 60 ? 320 : 60;
}

// ── Google Scholar external lookup ───────────────────────────────
function scholarAuthorUrl(displayName: string): string {
  const author = encodeURIComponent(`"${displayName}"`).replace(/%20/g, '+');
  return (
    'https://scholar.google.com/scholar' +
    '?as_q=&as_epq=&as_oq=&as_eq=&as_occt=any' +
    `&as_sauthors=${author}` +
    '&as_publication=&as_ylo=&as_yhi=&hl=en&as_sdt=0%2C34'
  );
}

function openScholar(displayName: string): void {
  openUrl(scholarAuthorUrl(displayName)).catch((err) =>
    console.error('Failed to open Google Scholar link:', err)
  );
}

function scholarTooltip(displayName: string): string {
  return `Search ${displayName} on Google Scholar`;
}

// ── Deep link to filtered article list ───────────────────────────
function viewAuthorArticles(displayName: string): void {
  void router.push({
    name: 'articles',
    query: { author: displayName, status: 'all', filterCollapsed: '1', from: 'authors' },
  });
}

function cellValue(author: AuthorRank, col: SortColumn, index: number): string | number {
  if (col.key === 'index') return index + 1;
  const val = author[col.key];
  if (val === null || val === undefined) return '—';
  if (typeof val === 'number') return col.label === 'Avg/Paper' ? val.toFixed(1) : val;
  return val;
}

// ── Sparkline chart options for detail panel ─────────────────────
const sparklineOptions = computed<ApexOptions>(() => ({
  chart: {
    type: 'bar',
    height: 80,
    toolbar: { show: false },
    animations: { enabled: false },
    fontFamily: 'inherit',
    background: 'transparent',
  },
  plotOptions: { bar: { columnWidth: '70%', borderRadius: 3, borderRadiusApplication: 'end' } },
  colors: ['#10b981'],
  dataLabels: { enabled: false },
  xaxis: {
    categories: detail.value?.pubsByYear.map((p) => String(p.year)) ?? [],
    labels: { style: { colors: '#94a3b8', fontSize: '10px' } },
    axisBorder: { show: false },
    axisTicks: { show: false },
  },
  yaxis: { labels: { show: false } },
  grid: { show: false },
  tooltip: { theme: 'light' },
}));

const sparklineSeries = computed(() => [
  { name: 'Publications', data: detail.value?.pubsByYear.map((p) => p.count) ?? [] },
]);

// ── Export (PNG / SVG) ───────────────────────────────────────────
async function handleExportPng(): Promise<void> {
  try {
    const chart = scatterRef.value;
    if (!chart) return;
    const result = await (
      chart as unknown as { dataURI: () => Promise<{ imgURI: string }> }
    ).dataURI();
    const filePath = await save({
      defaultPath: 'author-scatter.png',
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
    const chartEl = document.querySelector('.scatter-chart svg');
    if (!chartEl) return;
    const clone = chartEl.cloneNode(true) as SVGElement;
    clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
    clone.setAttribute('xmlns:xlink', 'http://www.w3.org/1999/xlink');
    const svgString = new XMLSerializer().serializeToString(clone);
    const filePath = await save({
      defaultPath: 'author-scatter.svg',
      filters: [{ name: 'SVG', extensions: ['svg'] }],
    });
    if (!filePath) return;
    await tauriCommand('write_text_to_file', { path: filePath, content: svgString });
  } catch (e) {
    console.error('SVG export failed', e);
  }
}

// ── CSV Export ───────────────────────────────────────────────────
async function handleExportCsv(): Promise<void> {
  try {
    const filePath = await save({
      defaultPath: 'author-rankings.csv',
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    });
    if (!filePath) return;

    const headers = [
      'Rank',
      'Author',
      'Papers',
      'Citations',
      'h-index',
      'i10',
      'g-index',
      'First Author',
      'Last Author',
      'Solo',
      'Avg Citations/Paper',
      'Avg Year',
      'Recent (5y)',
      'Institution',
    ];
    const lines: string[] = [headers.join(',')];

    filteredRankings.value.forEach((a, idx) => {
      const row = [
        String(idx + 1),
        `"${a.displayName.replace(/"/g, '""')}"`,
        String(a.articleCount),
        String(a.totalCitations),
        String(a.estimatedHIndex),
        String(a.i10Index),
        String(a.gIndex),
        String(a.firstAuthorCount),
        String(a.lastAuthorCount),
        String(a.soloPaperCount),
        a.avgCitationsPerPaper !== null ? a.avgCitationsPerPaper.toFixed(1) : '',
        a.avgYear !== null ? String(a.avgYear) : '',
        String(a.recentPaperCount),
        a.primaryInstitution ? `"${a.primaryInstitution.replace(/"/g, '""')}"` : '',
      ];
      lines.push(row.join(','));
    });

    await tauriCommand('write_text_to_file', { path: filePath, content: lines.join('\n') });
  } catch (e) {
    console.error('CSV export failed', e);
  }
}

// ── Keyboard navigation ──────────────────────────────────────────
function onRowKeydown(event: KeyboardEvent, author: AuthorRank): void {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault();
    selectAuthor(author);
  }
}

function onPanelKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault();
    closeDetail();
  }
}
</script>

<template>
  <div class="authors-layout">
    <!-- Error state -->
    <div v-if="error" class="authors-error">
      <span class="material-symbols-outlined">error</span>
      <p>{{ error }}</p>
    </div>

    <!-- Empty state -->
    <div v-else-if="!loading && rankings.length === 0" class="authors-empty">
      <span class="material-symbols-outlined authors-empty__icon">inbox</span>
      <p class="authors-empty__text">
        No articles yet. Import and review articles to see author productivity rankings.
      </p>
    </div>

    <template v-else>
      <!-- ── Sidebar ───────────────────────────────────────── -->
      <div class="sidebar-wrapper" :class="sidebarCollapsed ? 'w-0' : 'w-64'">
        <aside class="sidebar" :class="{ 'sidebar--collapsed': sidebarCollapsed }">
          <div class="sidebar__scroll">
            <!-- Min papers -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Min Papers</h4>
              <input v-model.number="minPapers" type="number" min="1" class="sidebar__input" />
            </section>

            <!-- Top N -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Top N</h4>
              <select v-model.number="topN" class="sidebar__select">
                <option :value="0">All</option>
                <option :value="25">25</option>
                <option :value="50">50</option>
                <option :value="100">100</option>
              </select>
            </section>

            <!-- Year range -->
            <section v-if="yearFrom !== null && yearTo !== null" class="sidebar__section">
              <h4 class="sidebar__label">Avg Year Range</h4>
              <div class="dual-range-block">
                <div class="dual-range-header">
                  <span class="dual-range-value">{{ yearFrom }} – {{ yearTo }}</span>
                  <button
                    v-if="yearFrom !== yearMin || yearTo !== yearMax"
                    class="dual-range-reset"
                    @click="
                      yearFrom = yearMin;
                      yearTo = yearMax;
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
                      left: `${((yearFrom - yearMin) / Math.max(1, yearMax - yearMin)) * 100}%`,
                      right: `${((yearMax - yearTo) / Math.max(1, yearMax - yearMin)) * 100}%`,
                    }"
                  ></div>
                  <input
                    v-model.number="yearFrom"
                    type="range"
                    :min="yearMin"
                    :max="yearMax"
                    step="1"
                    class="dual-range-input"
                    aria-label="Year from"
                  />
                  <input
                    v-model.number="yearTo"
                    type="range"
                    :min="yearMin"
                    :max="yearMax"
                    step="1"
                    class="dual-range-input"
                    aria-label="Year to"
                  />
                </div>
                <div class="dual-range-endpoints">
                  <span>{{ yearMin }}</span>
                  <span>{{ yearMax }}</span>
                </div>
              </div>
            </section>

            <!-- Stats -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Stats</h4>
              <p class="sidebar__stat-line">{{ filteredRankings.length }} authors shown</p>
              <p class="sidebar__stat-line">{{ rankings.length }} total authors</p>
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
              <button class="sidebar__export" @click="handleExportCsv">
                <span class="material-symbols-outlined">table_view</span>
                Export CSV
              </button>
            </section>

            <!-- Legend -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Metrics</h4>
              <p class="sidebar__hint"><strong>h-index:</strong> h papers cited h times each.</p>
              <p class="sidebar__hint"><strong>i10:</strong> papers with ≥ 10 citations.</p>
              <p class="sidebar__hint">
                <strong>g-index:</strong> top-n papers have ≥ n² cumulative citations (est.).
              </p>
              <p class="sidebar__hint" style="margin-top: 0.5rem">
                <strong>Scatter:</strong> X=Papers, Y=Citations, Size=h, Color=avg year
              </p>
            </section>
          </div>
        </aside>
        <button
          class="drawer-handle"
          :title="sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'"
          :style="{ left: sidebarCollapsed ? '0px' : 'calc(100% - 16px)' }"
          @click="sidebarCollapsed = !sidebarCollapsed"
        >
          <span class="drawer-handle-grip"></span>
        </button>
      </div>

      <!-- ── Main canvas ────────────────────────────────────── -->
      <main class="authors-main">
        <!-- KPI strip -->
        <section class="kpi-strip">
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ kpis?.totalAuthors ?? '—' }}</span>
            <span class="kpi-mini__label">Authors</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ kpis?.totalPapers.toLocaleString() ?? '—' }}</span>
            <span class="kpi-mini__label">Total Papers</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">
              {{
                kpis?.avgHIndex !== null && kpis?.avgHIndex !== undefined
                  ? kpis.avgHIndex.toFixed(1)
                  : '—'
              }}
            </span>
            <span class="kpi-mini__label">Avg h-index</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">{{ kpis?.maxHIndex ?? '—' }}</span>
            <span class="kpi-mini__label">Max h-index</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">
              {{
                kpis?.avgCitations !== null && kpis?.avgCitations !== undefined
                  ? kpis.avgCitations.toFixed(1)
                  : '—'
              }}
            </span>
            <span class="kpi-mini__label">Avg Cites</span>
          </div>
          <div class="kpi-mini">
            <span class="kpi-mini__value">
              <template v-if="kpis?.yearFrom && kpis?.yearTo">
                {{ kpis.yearFrom }}–{{ kpis.yearTo }}
              </template>
              <template v-else>—</template>
            </span>
            <span class="kpi-mini__label">Years</span>
          </div>
        </section>

        <!-- Loading -->
        <div v-if="loading" class="chart-loading">
          <span class="material-symbols-outlined chart-spin">progress_activity</span>
        </div>

        <template v-else>
          <!-- Scatter plot -->
          <div
            v-if="filteredRankings.length > 0"
            class="scatter-section"
            :style="{ height: scatterHeight + 'px' }"
          >
            <h6 class="scatter-title">Productivity vs Impact</h6>
            <VueApexCharts
              :key="scatterKey"
              ref="scatterRef"
              type="bubble"
              :options="scatterOptions"
              :series="scatterSeries"
              :height="Math.max(40, scatterHeight - 24)"
              class="scatter-chart"
            />
          </div>

          <!-- Draggable splitter -->
          <div
            v-if="filteredRankings.length > 0"
            class="splitter"
            :class="{ 'splitter--dragging': isDragging }"
            title="Drag to resize · Double-click to toggle"
            @mousedown="onSplitterMouseDown"
            @dblclick="onSplitterDoubleClick"
          >
            <span class="splitter__grip"></span>
          </div>

          <!-- Ranking table -->
          <div v-if="filteredRankings.length > 0" class="table-container">
            <table class="ranking-table">
              <thead>
                <tr>
                  <th
                    v-for="col in columns"
                    :key="col.key"
                    :class="[
                      'ranking-table__th',
                      {
                        'ranking-table__th--num': col.numeric,
                        'ranking-table__th--sortable': col.key !== 'index',
                        'ranking-table__th--active': isSortActive(col),
                      },
                    ]"
                    @click="toggleSort(col)"
                  >
                    <span class="ranking-table__th-label">{{ col.label }}</span>
                    <span
                      v-if="col.key !== 'index'"
                      class="material-symbols-outlined ranking-table__sort-icon"
                      :class="{ 'ranking-table__sort-icon--active': isSortActive(col) }"
                    >
                      {{ getSortIcon(col) }}
                    </span>
                  </th>
                  <th class="ranking-table__th ranking-table__th--num">Institution</th>
                  <th class="ranking-table__th">Scholar</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="(author, idx) in filteredRankings"
                  :key="author.id"
                  :tabindex="0"
                  :class="[
                    'ranking-table__row',
                    { 'ranking-table__row--active': selectedAuthorId === author.id },
                  ]"
                  @click="selectAuthor(author)"
                  @keydown="onRowKeydown($event, author)"
                >
                  <td
                    v-for="col in columns"
                    :key="col.key"
                    :class="[
                      'ranking-table__td',
                      {
                        'ranking-table__td--num': col.numeric,
                        'ranking-table__td--name': col.key === 'displayName',
                      },
                    ]"
                  >
                    <template v-if="col.key === 'displayName'">
                      <span class="ranking-table__author" :title="author.displayName">
                        {{ author.displayName }}
                      </span>
                    </template>
                    <template v-else>
                      {{ cellValue(author, col, idx) }}
                    </template>
                  </td>
                  <td class="ranking-table__td ranking-table__td--inst">
                    <span :title="author.primaryInstitution ?? ''">
                      {{ author.primaryInstitution ?? '—' }}
                    </span>
                  </td>
                  <td class="ranking-table__td ranking-table__td--icon" @click.stop>
                    <button
                      class="scholar-btn"
                      :title="scholarTooltip(author.displayName)"
                      :aria-label="scholarTooltip(author.displayName)"
                      @click="openScholar(author.displayName)"
                    >
                      <span class="material-symbols-outlined">open_in_new</span>
                    </button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- No results after filter -->
          <div v-else class="chart-empty">
            <span class="material-symbols-outlined">filter_alt_off</span>
            <p>No authors match the current filters.</p>
          </div>
        </template>
      </main>

      <!-- ── Author Detail Panel ────────────────────────────── -->
      <Transition name="detail-slide">
        <aside v-if="selectedAuthorId" class="author-panel" tabindex="-1" @keydown="onPanelKeydown">
          <div v-if="detailLoading" class="author-panel__loading">
            <span class="material-symbols-outlined author-panel__spin">progress_activity</span>
          </div>
          <div v-else-if="!detail" class="author-panel__error">
            <span class="material-symbols-outlined">error</span>
            <p>Failed to load author details.</p>
          </div>
          <template v-else>
            <header class="author-panel__header">
              <div class="author-panel__title-row">
                <h3 class="author-panel__title" :title="detail.rank.displayName">
                  {{ detail.rank.displayName }}
                </h3>
                <button
                  class="author-panel__scholar"
                  :title="scholarTooltip(detail.rank.displayName)"
                  :aria-label="scholarTooltip(detail.rank.displayName)"
                  @click="openScholar(detail.rank.displayName)"
                >
                  <span class="material-symbols-outlined">open_in_new</span>
                </button>
                <button class="author-panel__close" title="Close" @click="closeDetail">
                  <span class="material-symbols-outlined">close</span>
                </button>
              </div>
            </header>
            <div class="author-panel__body">
              <div class="author-panel__metrics">
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.articleCount }}</span>
                  <span class="author-panel__metric-label">Papers</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.totalCitations }}</span>
                  <span class="author-panel__metric-label">Citations</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.estimatedHIndex }}</span>
                  <span class="author-panel__metric-label">h-index</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.i10Index }}</span>
                  <span class="author-panel__metric-label">i10</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.gIndex }}</span>
                  <span class="author-panel__metric-label">g-index</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.firstAuthorCount }}</span>
                  <span class="author-panel__metric-label">First</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">{{ detail.rank.lastAuthorCount }}</span>
                  <span class="author-panel__metric-label">Last</span>
                </div>
                <div class="author-panel__metric">
                  <span class="author-panel__metric-value">
                    {{
                      detail.rank.avgCitationsPerPaper !== null
                        ? detail.rank.avgCitationsPerPaper.toFixed(1)
                        : '—'
                    }}
                  </span>
                  <span class="author-panel__metric-label">Avg/Paper</span>
                </div>
              </div>
              <div v-if="detail.pubsByYear.length > 0" class="author-panel__sparkline">
                <h5 class="author-panel__subhead">Publications by Year</h5>
                <VueApexCharts
                  type="bar"
                  :options="sparklineOptions"
                  :series="sparklineSeries"
                  height="80"
                  class="author-panel__chart"
                />
              </div>
              <div v-if="detail.institutions.length > 0" class="author-panel__section">
                <h5 class="author-panel__subhead">Institutions</h5>
                <ul class="author-panel__inst-list">
                  <li v-for="inst in detail.institutions" :key="inst.id" class="author-panel__inst">
                    <span class="author-panel__inst-name">{{ inst.normalizedName }}</span>
                    <span v-if="inst.country || inst.city" class="author-panel__inst-loc">
                      {{ [inst.city, inst.country].filter(Boolean).join(', ') }}
                    </span>
                  </li>
                </ul>
              </div>
              <div v-if="detail.topCollaborators.length > 0" class="author-panel__section">
                <h5 class="author-panel__subhead">Top Collaborators</h5>
                <ul class="author-panel__collab-list">
                  <li
                    v-for="c in detail.topCollaborators"
                    :key="c.collaboratorId"
                    class="author-panel__collab"
                  >
                    <span class="author-panel__collab-name">{{ c.collaboratorName }}</span>
                    <span class="author-panel__collab-count">{{ c.sharedPapers }} shared</span>
                  </li>
                </ul>
              </div>
              <div v-if="detail.recentPapers.length > 0" class="author-panel__section">
                <h5 class="author-panel__subhead">Recent Papers</h5>
                <ul class="author-panel__paper-list">
                  <li
                    v-for="p in detail.recentPapers"
                    :key="p.articleId"
                    class="author-panel__paper"
                  >
                    <span class="author-panel__paper-title" :title="p.title">{{ p.title }}</span>
                    <span class="author-panel__paper-meta">
                      {{ p.publicationYear ?? '—' }} · {{ p.numCited ?? 0 }} cites
                    </span>
                  </li>
                </ul>
              </div>
              <button
                class="author-panel__view-btn"
                @click="viewAuthorArticles(detail.rank.displayName)"
              >
                <span class="material-symbols-outlined">article</span>
                View this author's articles
              </button>
            </div>
          </template>
        </aside>
      </Transition>
    </template>
  </div>
</template>

<style scoped>
.authors-layout {
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
  border-right: 1px solid var(--color-outline-variant);
  background: var(--color-surface-container-low);
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
  color: var(--color-outline);
  margin: 0;
}
.sidebar__select,
.sidebar__input {
  padding: 0.375rem 0.5rem;
  border: 1px solid var(--color-outline-variant);
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-family: inherit;
  background: var(--color-surface-container-lowest);
  color: var(--color-on-surface);
  outline: none;
  transition: border-color 0.15s;
}
.sidebar__select:focus,
.sidebar__input:focus {
  border-color: var(--color-primary);
  box-shadow: 0 0 0 1px var(--color-primary);
}
.sidebar__stat-line {
  font-size: 0.75rem;
  color: var(--color-on-surface-variant);
  margin: 0;
}
.sidebar__hint {
  font-size: 0.6875rem;
  color: var(--color-on-surface-variant);
  line-height: 1.5;
  margin: 0;
}
.sidebar__export {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.5rem;
  border: 1px solid var(--color-outline-variant);
  border-radius: 0.375rem;
  font-size: 0.75rem;
  font-family: inherit;
  background: var(--color-surface-container-lowest);
  color: var(--color-on-surface-variant);
  cursor: pointer;
  transition: border-color 0.15s;
}
.sidebar__export:hover {
  border-color: var(--color-primary);
  color: var(--color-primary);
}
.sidebar__export .material-symbols-outlined {
  font-size: 0.875rem;
}

/* Dual-handle year range */
.dual-range-block {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.dual-range-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.dual-range-value {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--color-on-surface-variant);
}
.dual-range-reset {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 0.6875rem;
  color: var(--color-primary);
  padding: 0;
}
.dual-range-track {
  position: relative;
  height: 24px;
}
.dual-range-bar-bg {
  position: absolute;
  top: 10px;
  left: 0;
  right: 0;
  height: 4px;
  background: var(--color-outline-variant);
  border-radius: 2px;
}
.dual-range-bar-active {
  position: absolute;
  top: 10px;
  height: 4px;
  background: var(--color-primary);
  border-radius: 2px;
}
.dual-range-input {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 24px;
  margin: 0;
  pointer-events: none;
  -webkit-appearance: none;
  appearance: none;
  background: none;
  outline: none;
}
.dual-range-input::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  pointer-events: auto;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--color-surface-container-lowest);
  border: 2px solid var(--color-primary);
  cursor: pointer;
}
.dual-range-input::-moz-range-thumb {
  pointer-events: auto;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--color-surface-container-lowest);
  border: 2px solid var(--color-primary);
  cursor: pointer;
}
.dual-range-endpoints {
  display: flex;
  justify-content: space-between;
  font-size: 0.625rem;
  color: var(--color-outline);
}

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
  background: var(--color-surface-container-low);
  border: 1px solid var(--color-outline-variant);
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
  background: var(--color-surface-container);
  border-color: var(--color-primary);
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
  background: var(--color-primary);
}

/* ── Main canvas ─────────────────────────────────────────────── */
.authors-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  overflow: hidden;
}

.kpi-strip {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 0.75rem;
  padding: 1rem;
  border-bottom: 1px solid var(--color-outline-variant);
  background: var(--color-surface-container-low);
  flex-shrink: 0;
}
@media (max-width: 1024px) {
  .kpi-strip {
    grid-template-columns: repeat(3, 1fr);
  }
}
@media (max-width: 640px) {
  .kpi-strip {
    grid-template-columns: repeat(2, 1fr);
  }
}
.kpi-mini {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  padding: 0.5rem;
  background: var(--color-surface-container-lowest);
  border: 1px solid var(--color-outline-variant);
  border-radius: 0.375rem;
}
.kpi-mini__value {
  font-size: 1.125rem;
  font-weight: 700;
  color: var(--color-primary);
  line-height: 1.2;
}
.kpi-mini__label {
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-outline);
  font-weight: 600;
}

.chart-loading,
.chart-empty,
.authors-empty,
.authors-error {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  color: var(--color-outline);
  padding: 2rem;
}
.authors-empty__icon {
  font-size: 3rem;
  opacity: 0.5;
}
.authors-empty__text {
  font-size: 0.875rem;
  color: var(--color-on-surface-variant);
  margin: 0;
}
.chart-spin,
.author-panel__spin {
  animation: authors-spin 1s linear infinite;
  font-size: 1.75rem;
  color: var(--color-primary);
}
@keyframes authors-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* ── Draggable splitter ─────────────────────────────────────── */
.splitter {
  height: 6px;
  cursor: row-resize;
  background: var(--color-surface-container-low);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  border-bottom: 1px solid var(--color-outline-variant);
  transition: background 0.15s;
  user-select: none;
  position: relative;
  z-index: 10;
}
/* Invisible grab zone extends 4px above/below the 6px bar for easier dragging */
.splitter::before {
  content: '';
  position: absolute;
  top: -4px;
  bottom: -4px;
  left: 0;
  right: 0;
}
.splitter:hover,
.splitter--dragging {
  background: var(--color-outline-variant);
}
.splitter__grip {
  width: 32px;
  height: 2px;
  background: var(--color-outline-variant);
  border-radius: 1px;
  transition: background 0.15s;
  pointer-events: none;
}
.splitter:hover .splitter__grip,
.splitter--dragging .splitter__grip {
  background: var(--color-primary);
}

/* ── Scatter plot ────────────────────────────────────────────── */
.scatter-section {
  padding: 0.5rem 1rem;
  flex-shrink: 0;
  overflow: hidden;
  border-bottom: 1px solid var(--color-outline-variant);
}
.scatter-title {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-outline);
  margin: 0 0 0.25rem 0;
}
.scatter-chart {
  width: 100%;
}

/* ── Ranking table ───────────────────────────────────────────── */
.table-container {
  flex: 1;
  overflow: auto;
  padding: 0 1rem 1rem;
}
.ranking-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.8125rem;
}
.ranking-table__th {
  text-align: left;
  padding: 0.625rem 0.5rem;
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-on-surface-variant);
  border-bottom: 1px solid var(--color-outline-variant);
  background: var(--color-surface-container-low);
  position: sticky;
  top: 0;
  z-index: 1;
}
.ranking-table__th--num {
  text-align: right;
}
.ranking-table__th--sortable {
  cursor: pointer;
  user-select: none;
  transition: color 0.1s;
}
.ranking-table__th--sortable:hover {
  color: var(--color-on-surface);
}
.ranking-table__th--active {
  color: var(--color-primary);
}
.ranking-table__th-label {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.ranking-table__sort-icon {
  font-size: 14px !important;
  color: var(--color-outline-variant);
  transition: color 0.1s;
}
.ranking-table__sort-icon--active {
  color: var(--color-primary);
}
.ranking-table__row {
  cursor: pointer;
  transition: background-color 0.1s;
  border-bottom: 1px solid var(--color-outline-variant);
}
.ranking-table__row:hover {
  background: var(--color-surface-container-low);
}
.ranking-table__row--active {
  background: var(--color-surface-container);
}
.ranking-table__row--active:hover {
  background: var(--color-surface-container-high);
}
.ranking-table__td {
  padding: 0.5rem;
  color: var(--color-on-surface);
  vertical-align: middle;
}
.ranking-table__td--num {
  text-align: right;
  font-variant-numeric: tabular-nums;
  font-weight: 500;
}
.ranking-table__td--name {
  font-weight: 500;
  max-width: 200px;
}
.ranking-table__author {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ranking-table__td--inst {
  font-size: 0.75rem;
  color: var(--color-on-surface-variant);
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ranking-table__td--icon {
  text-align: center;
  padding: 0.25rem;
}
.scholar-btn {
  background: none;
  border: none;
  cursor: pointer;
  padding: 0.25rem;
  border-radius: 0.25rem;
  color: var(--color-outline);
  display: inline-flex;
  transition:
    background-color 0.15s,
    color 0.15s;
}
.scholar-btn:hover {
  background: var(--color-surface-container-low);
  color: var(--color-primary);
}
.scholar-btn .material-symbols-outlined {
  font-size: 1rem;
}

/* ── Detail panel ────────────────────────────────────────────── */
.author-panel {
  position: relative;
  width: 20rem;
  flex-shrink: 0;
  background: var(--color-surface-container-lowest);
  border-left: 1px solid var(--color-outline-variant);
  box-shadow: -8px 0 24px rgba(0, 0, 0, 0.08);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.author-panel__loading,
.author-panel__error {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  padding: 2rem 1rem;
  color: var(--color-outline);
  flex: 1;
}
.author-panel__header {
  display: flex;
  align-items: center;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid var(--color-outline-variant);
  flex-shrink: 0;
}
.author-panel__title-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
  min-width: 0;
}
.author-panel__title {
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-on-surface);
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
.author-panel__scholar,
.author-panel__close {
  background: none;
  border: none;
  cursor: pointer;
  padding: 0.25rem;
  border-radius: 0.25rem;
  color: var(--color-outline);
  display: flex;
  transition:
    background-color 0.15s,
    color 0.15s;
  flex-shrink: 0;
}
.author-panel__scholar:hover,
.author-panel__close:hover {
  background: var(--color-surface-container-low);
  color: var(--color-primary);
}
.author-panel__scholar .material-symbols-outlined,
.author-panel__close .material-symbols-outlined {
  font-size: 1.125rem;
}
.author-panel__body {
  flex: 1;
  overflow-y: auto;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}
.author-panel__metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.5rem;
}
.author-panel__metric {
  background: var(--color-surface-container-low);
  border-radius: 0.375rem;
  padding: 0.5rem;
  text-align: center;
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}
.author-panel__metric-value {
  font-size: 1rem;
  font-weight: 700;
  color: var(--color-primary);
  line-height: 1.1;
}
.author-panel__metric-label {
  font-size: 0.5625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-outline);
  font-weight: 600;
}
.author-panel__subhead {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: var(--color-outline);
  margin: 0 0 0.5rem 0;
}
.author-panel__section {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.author-panel__inst-list,
.author-panel__collab-list,
.author-panel__paper-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}
.author-panel__inst,
.author-panel__collab,
.author-panel__paper {
  font-size: 0.75rem;
  color: var(--color-on-surface-variant);
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}
.author-panel__inst-name {
  font-weight: 500;
  color: var(--color-on-surface);
}
.author-panel__inst-loc {
  font-size: 0.6875rem;
  color: var(--color-on-surface-variant);
}
.author-panel__collab {
  flex-direction: row;
  justify-content: space-between;
  align-items: baseline;
}
.author-panel__collab-count {
  font-size: 0.6875rem;
  color: var(--color-primary);
  font-weight: 600;
}
.author-panel__paper-title {
  font-weight: 500;
  color: var(--color-on-surface);
  line-height: 1.4;
}
.author-panel__paper-meta {
  font-size: 0.6875rem;
  color: var(--color-on-surface-variant);
}
.author-panel__view-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  background: var(--color-primary);
  color: #ffffff;
  border: none;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: opacity 0.15s;
  margin-top: auto;
}
.author-panel__view-btn:hover {
  opacity: 0.9;
}
.author-panel__view-btn .material-symbols-outlined {
  font-size: 1.125rem;
}

/* Slide transition */
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

/* ── Responsive: Detail panel as overlay on narrow screens ──── */
@media (max-width: 768px) {
  .author-panel {
    position: absolute;
    top: 0;
    right: 0;
    height: 100%;
    width: 100%;
    max-width: 24rem;
    z-index: 20;
  }
}
@media (max-width: 480px) {
  .author-panel {
    max-width: 100%;
  }
  .author-panel__metrics {
    grid-template-columns: repeat(2, 1fr);
  }
  .ranking-table {
    font-size: 0.75rem;
  }
  .ranking-table__th,
  .ranking-table__td {
    padding: 0.375rem 0.25rem;
  }
  .ranking-table__td--name {
    max-width: 120px;
  }
  .ranking-table__td--inst {
    max-width: 100px;
  }
}

/* Keyboard focus visibility for accessibility */
.ranking-table__row:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: -2px;
}
.author-panel:focus-visible {
  outline: none;
}
</style>
