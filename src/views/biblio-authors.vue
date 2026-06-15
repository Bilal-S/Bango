<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { openUrl } from '@tauri-apps/plugin-opener';
import VueApexCharts from 'vue3-apexcharts';
import type { ApexOptions } from 'apexcharts';
import { useAuthorRankings } from '@/composables/use-author-rankings';
import type { AuthorRank } from '@/composables/use-author-rankings';
import { useAuthorDetail } from '@/composables/use-author-detail';

const router = useRouter();
const { rankings, kpis, loading, error } = useAuthorRankings();
const { detail, loading: detailLoading, getAuthorDetail, clear } = useAuthorDetail();

// ── Sidebar state ────────────────────────────────────────────────
const sidebarCollapsed = ref(false);
const minPapers = ref(1);

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
  if (col.key === 'index') return; // rank # column is not sortable
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

// ── Filtered + sorted rankings ───────────────────────────────────
const filteredRankings = computed(() => {
  const list = rankings.value.filter((a) => a.articleCount >= minPapers.value);
  const col = sortColumn.value;
  const dir = sortDirection.value === 'asc' ? 1 : -1;
  return [...list].sort((a, b) => {
    const av = a[col];
    const bv = b[col];
    if (typeof av === 'number' && typeof bv === 'number') {
      return (av - bv) * dir;
    }
    return String(av).localeCompare(String(bv)) * dir;
  });
});

// ── Selected author for detail panel ─────────────────────────────
const selectedAuthorId = ref<string | null>(null);

watch(selectedAuthorId, (id) => {
  if (id) {
    void getAuthorDetail(id);
  } else {
    clear();
  }
});

function selectAuthor(author: AuthorRank): void {
  // Toggle: clicking the same author closes the panel
  selectedAuthorId.value = selectedAuthorId.value === author.id ? null : author.id;
}

function closeDetail(): void {
  selectedAuthorId.value = null;
}

// ── Google Scholar external lookup ───────────────────────────────
/**
 * Build a Google Scholar advanced-search URL scoped to a single author.
 * Uses `as_sauthors` (author field) with a quoted exact-match phrase.
 */
function scholarAuthorUrl(displayName: string): string {
  // encodeURIComponent encodes space as %20, but Google Scholar query params
  // expect spaces as + (form-encoding). Replace after encoding.
  const author = encodeURIComponent(`"${displayName}"`).replace(/%20/g, '+');
  return (
    'https://scholar.google.com/scholar' +
    '?as_q=&as_epq=&as_oq=&as_eq=&as_occt=any' +
    `&as_sauthors=${author}` +
    '&as_publication=&as_ylo=&as_yhi=&hl=en&as_sdt=0%2C34'
  );
}

function openScholar(displayName: string): void {
  openUrl(scholarAuthorUrl(displayName)).catch((err) => {
    console.error('Failed to open Google Scholar link:', err);
  });
}

/** Build the Scholar tooltip without inline quotes (avoids Vue template parsing issues). */
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
  if (typeof val === 'number') {
    return col.label === 'Avg/Paper' ? val.toFixed(1) : val;
  }
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
  plotOptions: {
    bar: { columnWidth: '70%', borderRadius: 3, borderRadiusApplication: 'end' },
  },
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
        No included articles yet. Include articles to see author productivity rankings.
      </p>
    </div>

    <template v-else>
      <!-- ── Sidebar ───────────────────────────────────────── -->
      <div class="sidebar-wrapper" :class="sidebarCollapsed ? 'w-0' : 'w-64'">
        <aside class="sidebar" :class="{ 'sidebar--collapsed': sidebarCollapsed }">
          <div class="sidebar__scroll">
            <!-- Min papers filter -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Min Papers</h4>
              <input v-model.number="minPapers" type="number" min="1" class="sidebar__input" />
            </section>

            <!-- Stats -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Stats</h4>
              <p class="sidebar__stat-line">{{ filteredRankings.length }} authors shown</p>
              <p class="sidebar__stat-line">{{ rankings.length }} total authors</p>
            </section>

            <!-- Legend -->
            <section class="sidebar__section">
              <h4 class="sidebar__label">Metrics</h4>
              <p class="sidebar__hint"><strong>h-index:</strong> h papers cited h times each.</p>
              <p class="sidebar__hint"><strong>i10:</strong> papers with ≥ 10 citations.</p>
              <p class="sidebar__hint">
                <strong>g-index:</strong> top-n papers have ≥ n² cumulative citations (est.).
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

        <!-- Ranking table -->
        <div v-else-if="filteredRankings.length > 0" class="table-container">
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
                :class="[
                  'ranking-table__row',
                  { 'ranking-table__row--active': selectedAuthorId === author.id },
                ]"
                @click="selectAuthor(author)"
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
      </main>

      <!-- ── Author Detail Panel ────────────────────────────── -->
      <Transition name="detail-slide">
        <aside v-if="selectedAuthorId" class="author-panel">
          <!-- Loading -->
          <div v-if="detailLoading" class="author-panel__loading">
            <span class="material-symbols-outlined author-panel__spin">progress_activity</span>
          </div>

          <!-- Error -->
          <div v-else-if="!detail" class="author-panel__error">
            <span class="material-symbols-outlined">error</span>
            <p>Failed to load author details.</p>
          </div>

          <!-- Detail content -->
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
              <!-- Metrics grid -->
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

              <!-- Productivity sparkline -->
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

              <!-- Institutions -->
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

              <!-- Top collaborators -->
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

              <!-- Recent papers -->
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

              <!-- Deep link button -->
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
  color: #94a3b8;
  margin: 0;
}

.sidebar__select,
.sidebar__input {
  padding: 0.375rem 0.5rem;
  border: 1px solid #e2e8f0;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-family: inherit;
  background: #fff;
  color: #1e293b;
  outline: none;
  transition: border-color 0.15s;
}

.sidebar__select:focus,
.sidebar__input:focus {
  border-color: var(--color-primary, #4f46e5);
  box-shadow: 0 0 0 1px var(--color-primary, #4f46e5);
}

.sidebar__stat-line {
  font-size: 0.75rem;
  color: #475569;
  margin: 0;
}

.sidebar__hint {
  font-size: 0.6875rem;
  color: #64748b;
  line-height: 1.5;
  margin: 0;
}

.drawer-handle {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  width: 16px;
  height: 48px;
  background: #e2e8f0;
  border: none;
  border-radius: 0 4px 4px 0;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
}

.drawer-handle:hover {
  background: #cbd5e1;
}

.drawer-handle-grip {
  width: 2px;
  height: 16px;
  background: #94a3b8;
  border-radius: 1px;
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
  border-bottom: 1px solid #f1f5f9;
  background: #fafbfc;
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
  background: #fff;
  border: 1px solid #f1f5f9;
  border-radius: 0.375rem;
}

.kpi-mini__value {
  font-size: 1.125rem;
  font-weight: 700;
  color: var(--color-primary, #4f46e5);
  line-height: 1.2;
}

.kpi-mini__label {
  font-size: 0.625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
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
  color: #94a3b8;
  padding: 2rem;
}

.authors-empty__icon {
  font-size: 3rem;
  opacity: 0.5;
}

.authors-empty__text {
  font-size: 0.875rem;
  color: #64748b;
  margin: 0;
}

.chart-spin,
.author-panel__spin {
  animation: authors-spin 1s linear infinite;
  font-size: 1.75rem;
  color: var(--color-primary, #4f46e5);
}

@keyframes authors-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
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
  color: #64748b;
  border-bottom: 1px solid #e2e8f0;
  background: #fafbfc;
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
  color: #1e293b;
}

.ranking-table__th--active {
  color: var(--color-primary, #4f46e5);
}

.ranking-table__th-label {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.ranking-table__sort-icon {
  font-size: 14px !important;
  color: #cbd5e1;
  transition: color 0.1s;
}

.ranking-table__sort-icon--active {
  color: var(--color-primary, #4f46e5);
}

.ranking-table__row {
  cursor: pointer;
  transition: background-color 0.1s;
  border-bottom: 1px solid #f1f5f9;
}

.ranking-table__row:hover {
  background: #f8fafc;
}

.ranking-table__row--active {
  background: #eef2ff;
}

.ranking-table__row--active:hover {
  background: #e0e7ff;
}

.ranking-table__td {
  padding: 0.5rem;
  color: #1e293b;
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
  color: #64748b;
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
  color: #94a3b8;
  display: inline-flex;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.scholar-btn:hover {
  background: #f1f5f9;
  color: var(--color-primary, #4f46e5);
}

.scholar-btn .material-symbols-outlined {
  font-size: 1rem;
}

/* ── Detail panel ────────────────────────────────────────────── */
.author-panel {
  position: relative;
  width: 20rem;
  flex-shrink: 0;
  background: #ffffff;
  border-left: 1px solid var(--color-border, #e2e8f0);
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
  color: #94a3b8;
  flex: 1;
}

.author-panel__header {
  display: flex;
  align-items: center;
  padding: 0.75rem 1rem;
  border-bottom: 1px solid #f1f5f9;
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
  color: #1e293b;
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
  color: #94a3b8;
  display: flex;
  transition:
    background-color 0.15s,
    color 0.15s;
  flex-shrink: 0;
}

.author-panel__scholar:hover,
.author-panel__close:hover {
  background: #f1f5f9;
  color: var(--color-primary, #4f46e5);
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
  background: #f8fafc;
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
  color: var(--color-primary, #4f46e5);
  line-height: 1.1;
}

.author-panel__metric-label {
  font-size: 0.5625rem;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
  font-weight: 600;
}

.author-panel__subhead {
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.03em;
  color: #94a3b8;
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
  color: #334155;
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.author-panel__inst-name {
  font-weight: 500;
  color: #1e293b;
}

.author-panel__inst-loc {
  font-size: 0.6875rem;
  color: #64748b;
}

.author-panel__collab {
  flex-direction: row;
  justify-content: space-between;
  align-items: baseline;
}

.author-panel__collab-count {
  font-size: 0.6875rem;
  color: var(--color-primary, #4f46e5);
  font-weight: 600;
}

.author-panel__paper-title {
  font-weight: 500;
  color: #1e293b;
  line-height: 1.4;
}

.author-panel__paper-meta {
  font-size: 0.6875rem;
  color: #64748b;
}

.author-panel__view-btn {
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
  transition: opacity 0.15s;
  margin-top: auto;
}

.author-panel__view-btn:hover {
  opacity: 0.9;
}

.author-panel__view-btn .material-symbols-outlined {
  font-size: 1.125rem;
}

/* Slide transition (matches journal-info-card.vue) */
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
