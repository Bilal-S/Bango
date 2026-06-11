<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { useBibliometrics } from '../composables/use-bibliometrics';

const router = useRouter();

const { kpis, loading, normalizing, runNormalization } = useBibliometrics();

const includedCount = computed(() => kpis.value.includedCount);
const totalCitations = computed(() => kpis.value.totalCitations);
const uniqueAuthors = computed(() => kpis.value.uniqueAuthors);
const avgCitationsPerArticle = computed(() =>
  kpis.value.includedCount > 0
    ? (kpis.value.totalCitations / kpis.value.includedCount).toFixed(1)
    : '—'
);
const avgGrowthRate = computed(() =>
  kpis.value.avgGrowthRate !== null
    ? `${kpis.value.avgGrowthRate >= 0 ? '+' : ''}${kpis.value.avgGrowthRate.toFixed(1)}%`
    : '—'
);
const yearFrom = computed(() => kpis.value.yearFrom ?? '—');
const yearTo = computed(() => kpis.value.yearTo ?? '—');
const totalPubs = computed(() => kpis.value.pubsByYear.reduce((sum, yc) => sum + yc.count, 0));
const maxPubsCount = computed(() => Math.max(1, ...kpis.value.pubsByYear.map((yc) => yc.count)));
const hasPubsByYear = computed(() => kpis.value.pubsByYear.length > 0);
const hasRefsByYear = computed(() => kpis.value.refsByYear.length > 0);
const totalReferences = computed(() =>
  kpis.value.refsByYear.reduce((sum, yc) => sum + yc.count, 0)
);
const maxRefsCount = computed(() => Math.max(1, ...kpis.value.refsByYear.map((yc) => yc.count)));
const firstRefYear = computed(() =>
  kpis.value.refsByYear.length > 0 ? (kpis.value.refsByYear[0]?.year ?? null) : null
);
const lastRefYear = computed(() =>
  kpis.value.refsByYear.length > 0
    ? (kpis.value.refsByYear[kpis.value.refsByYear.length - 1]?.year ?? null)
    : null
);
const hasCitationsByYear = computed(() => kpis.value.citationsByYear.length > 0);
const maxCitationsCount = computed(() =>
  Math.max(1, ...kpis.value.citationsByYear.map((yc) => yc.count))
);
const firstCitYear = computed(() =>
  kpis.value.citationsByYear.length > 0 ? (kpis.value.citationsByYear[0]?.year ?? null) : null
);
const lastCitYear = computed(() =>
  kpis.value.citationsByYear.length > 0
    ? (kpis.value.citationsByYear[kpis.value.citationsByYear.length - 1]?.year ?? null)
    : null
);
const showNoArticlesModal = computed(
  () => !loading.value && !normalizing.value && includedCount.value === 0
);
const firstYear = computed(() =>
  kpis.value.pubsByYear.length > 0 ? (kpis.value.pubsByYear[0]?.year ?? null) : null
);
const lastYear = computed(() =>
  kpis.value.pubsByYear.length > 0
    ? (kpis.value.pubsByYear[kpis.value.pubsByYear.length - 1]?.year ?? null)
    : null
);

// Growth-rate sparkline: derive year-over-year growth from pubsByYear
interface GrowthYear {
  year: number;
  rate: number;
}
const growthByYear = computed<GrowthYear[]>(() => {
  const py = kpis.value.pubsByYear;
  if (py.length < 2) return [];
  const result: GrowthYear[] = [];
  for (let i = 1; i < py.length; i++) {
    const prev = py[i - 1]!.count;
    const curr = py[i]!.count;
    const rate = prev > 0 ? ((curr - prev) / prev) * 100 : 0;
    result.push({ year: py[i]!.year, rate });
  }
  return result;
});
const hasGrowthByYear = computed(() => growthByYear.value.length > 0);
const maxGrowthAbs = computed(() =>
  Math.max(1, ...growthByYear.value.map((g) => Math.abs(g.rate)))
);
const firstGrowthYear = computed(() =>
  growthByYear.value.length > 0 ? growthByYear.value[0]!.year : null
);
const lastGrowthYear = computed(() =>
  growthByYear.value.length > 0 ? growthByYear.value[growthByYear.value.length - 1]!.year : null
);

interface AnalysisModule {
  id: string;
  label: string;
  description: string;
  icon: string;
  color: string;
}

const analysisModules: AnalysisModule[] = [
  {
    id: 'co-authorship',
    label: 'Co-Authorship Network',
    description: 'Map collaborative relationships between authors',
    icon: 'group',
    color: '#6366f1',
  },
  {
    id: 'citation-network',
    label: 'Citation Network',
    description: 'Visualize how articles cite each other',
    icon: 'account_tree',
    color: '#8b5cf6',
  },
  {
    id: 'keyword-cooccurrence',
    label: 'Keyword Co-Occurrence',
    description: 'Discover clusters of related research topics',
    icon: 'cloud',
    color: '#ec4899',
  },
  {
    id: 'publication-timeline',
    label: 'Publication Timeline',
    description: 'Track publishing trends over time',
    icon: 'timeline',
    color: '#f59e0b',
  },
  {
    id: 'author-productivity',
    label: 'Author Productivity',
    description: 'Rank authors by output and impact',
    icon: 'bar_chart',
    color: '#10b981',
  },
  {
    id: 'co-citation',
    label: 'Co-Citation Analysis',
    description: 'Find works frequently cited together',
    icon: 'hub',
    color: '#3b82f6',
  },
  {
    id: 'source-impact',
    label: 'Source Impact',
    description: 'Compare journal and conference influence',
    icon: 'campaign',
    color: '#f97316',
  },
];

function navigateToModule(mod: AnalysisModule): void {
  // For now, just show the module ID in console — sub-routes will be added later
  // eslint-disable-next-line no-console
  console.log(`Navigate to module: ${mod.id}`);
}

function goHome(): void {
  router.push('/');
}

function dismissModal(): void {
  router.push({ name: 'dashboard' });
}
</script>

<template>
  <div class="biblio">
    <!-- No Articles Modal Overlay -->
    <Teleport to="body">
      <div v-if="showNoArticlesModal" class="biblio-overlay" @click.self="dismissModal">
        <div class="biblio-modal">
          <span class="material-symbols-outlined biblio-modal__icon">info</span>
          <h2 class="biblio-modal__title">No Articles Included</h2>
          <p class="biblio-modal__message">
            Include articles in your research first to access the Bibliometric dashboard.
          </p>
          <button class="biblio-modal__btn" @click="dismissModal">OK</button>
        </div>
      </div>
    </Teleport>

    <!-- Page Header -->
    <section class="biblio__header">
      <div class="biblio__header-text">
        <h1 class="page-title">
          <button class="biblio__title-link" @click="goHome">Bibliometrics</button>
        </h1>
        <p class="biblio__subtitle">
          Analysis of {{ includedCount }} included articles from {{ yearFrom }} to {{ yearTo }}
        </p>
      </div>
      <!-- Progress bar while normalizing, Refresh button otherwise -->
      <Transition name="biblio-fade" mode="out-in">
        <div v-if="normalizing" key="progress" class="biblio__progress-wrap">
          <span class="material-symbols-outlined biblio__spin biblio__progress-icon">
            progress_activity
          </span>
          <span class="biblio__progress-text">Normalizing…</span>
          <div class="biblio__progress-bar">
            <div class="biblio__progress-fill"></div>
          </div>
        </div>
        <button v-else key="refresh" class="biblio__refresh-btn" @click="runNormalization">
          <span class="material-symbols-outlined">sync</span>
          Refresh
        </button>
      </Transition>
    </section>

    <!-- KPI Row — compact horizontal layout from High-Contrast Research Hub -->
    <section class="biblio__kpis">
      <div class="kpi-card kpi-card--chart">
        <div class="kpi-card__row">
          <div class="kpi-card__icon kpi-card__icon--blue">
            <span class="material-symbols-outlined">description</span>
          </div>
          <span
            v-if="loading || normalizing"
            class="material-symbols-outlined kpi-card__spinner biblio__spin"
            >progress_activity</span
          >
          <span v-else class="kpi-card__value">{{ totalReferences.toLocaleString() }}</span>
        </div>
        <div v-if="hasRefsByYear && !loading && !normalizing" class="kpi-sparkline">
          <div class="kpi-sparkline__bars">
            <div v-for="yc in kpis.refsByYear" :key="yc.year" class="kpi-sparkline__bar-wrap">
              <div
                class="kpi-sparkline__bar kpi-sparkline__bar--blue"
                :style="{ height: (yc.count / maxRefsCount) * 100 + '%' }"
              >
                <span class="kpi-sparkline__tooltip">{{ yc.year }}: {{ yc.count }}</span>
              </div>
            </div>
          </div>
          <div v-if="firstRefYear !== null && lastRefYear !== null" class="kpi-sparkline__years">
            <span>{{ firstRefYear }}</span>
            <span>{{ lastRefYear }}</span>
          </div>
        </div>
        <span class="kpi-card__label kpi-card__label--center">References</span>
      </div>
      <div class="kpi-card kpi-card--chart">
        <div class="kpi-card__row">
          <div class="kpi-card__icon kpi-card__icon--purple">
            <span class="material-symbols-outlined">format_quote</span>
          </div>
          <span
            v-if="loading || normalizing"
            class="material-symbols-outlined kpi-card__spinner biblio__spin"
            >progress_activity</span
          >
          <span v-else class="kpi-card__value">{{ totalCitations.toLocaleString() }}</span>
        </div>
        <div v-if="hasCitationsByYear && !loading && !normalizing" class="kpi-sparkline">
          <div class="kpi-sparkline__bars">
            <div v-for="yc in kpis.citationsByYear" :key="yc.year" class="kpi-sparkline__bar-wrap">
              <div
                class="kpi-sparkline__bar kpi-sparkline__bar--purple"
                :style="{ height: (yc.count / maxCitationsCount) * 100 + '%' }"
              >
                <span class="kpi-sparkline__tooltip">{{ yc.year }}: {{ yc.count }}</span>
              </div>
            </div>
          </div>
          <div v-if="firstCitYear !== null && lastCitYear !== null" class="kpi-sparkline__years">
            <span>{{ firstCitYear }}</span>
            <span>{{ lastCitYear }}</span>
          </div>
        </div>
        <span class="kpi-card__label kpi-card__label--center">Normalized Citations</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__row">
          <div class="kpi-card__icon kpi-card__icon--teal">
            <span class="material-symbols-outlined">group</span>
          </div>
          <span
            v-if="loading || normalizing"
            class="material-symbols-outlined kpi-card__spinner biblio__spin"
            >progress_activity</span
          >
          <span v-else class="kpi-card__value">{{ uniqueAuthors.toLocaleString() }}</span>
        </div>
        <span class="kpi-card__label">Unique Authors</span>
      </div>
      <div class="kpi-card kpi-card--chart">
        <div class="kpi-card__row">
          <div class="kpi-card__icon kpi-card__icon--amber">
            <span class="material-symbols-outlined">calendar_month</span>
          </div>
          <span
            v-if="loading || normalizing"
            class="material-symbols-outlined kpi-card__spinner biblio__spin"
            >progress_activity</span
          >
          <span v-else class="kpi-card__value">{{ totalPubs.toLocaleString() }}</span>
        </div>
        <div v-if="hasPubsByYear && !loading && !normalizing" class="kpi-sparkline">
          <div class="kpi-sparkline__bars">
            <div v-for="yc in kpis.pubsByYear" :key="yc.year" class="kpi-sparkline__bar-wrap">
              <div
                class="kpi-sparkline__bar"
                :style="{ height: (yc.count / maxPubsCount) * 100 + '%' }"
              >
                <span class="kpi-sparkline__tooltip">{{ yc.year }}: {{ yc.count }}</span>
              </div>
            </div>
          </div>
          <div v-if="firstYear !== null && lastYear !== null" class="kpi-sparkline__years">
            <span>{{ firstYear }}</span>
            <span>{{ lastYear }}</span>
          </div>
        </div>
        <span class="kpi-card__label kpi-card__label--center">Pubs / Year</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__row">
          <div class="kpi-card__icon kpi-card__icon--pink">
            <span class="material-symbols-outlined">star</span>
          </div>
          <span
            v-if="loading || normalizing"
            class="material-symbols-outlined kpi-card__spinner biblio__spin"
            >progress_activity</span
          >
          <span v-else class="kpi-card__value">{{ avgCitationsPerArticle }}</span>
        </div>
        <span class="kpi-card__label">Avg Citations / Article</span>
      </div>
      <div class="kpi-card kpi-card--chart">
        <div class="kpi-card__row">
          <div class="kpi-card__icon kpi-card__icon--green">
            <span class="material-symbols-outlined">trending_up</span>
          </div>
          <span
            v-if="loading || normalizing"
            class="material-symbols-outlined kpi-card__spinner biblio__spin"
            >progress_activity</span
          >
          <span v-else class="kpi-card__value">{{ avgGrowthRate }}</span>
        </div>
        <div v-if="hasGrowthByYear && !loading && !normalizing" class="kpi-sparkline">
          <div class="kpi-sparkline__bars">
            <div v-for="g in growthByYear" :key="g.year" class="kpi-sparkline__bar-wrap">
              <div
                class="kpi-sparkline__bar kpi-sparkline__bar--green"
                :style="{ height: (Math.abs(g.rate) / maxGrowthAbs) * 100 + '%' }"
              >
                <span class="kpi-sparkline__tooltip"
                  >{{ g.year }}: {{ g.rate >= 0 ? '+' : '' }}{{ g.rate.toFixed(1) }}%</span
                >
              </div>
            </div>
          </div>
          <div
            v-if="firstGrowthYear !== null && lastGrowthYear !== null"
            class="kpi-sparkline__years"
          >
            <span>{{ firstGrowthYear }}</span>
            <span>{{ lastGrowthYear }}</span>
          </div>
        </div>
        <span class="kpi-card__label kpi-card__label--center">Avg Growth Rate</span>
      </div>
    </section>

    <!-- Analysis Modules -->
    <section class="biblio__modules">
      <h2 class="biblio__section-label">Analysis Modules</h2>
      <div class="biblio__module-grid">
        <button
          v-for="mod in analysisModules"
          :key="mod.id"
          class="module-card"
          @click="navigateToModule(mod)"
        >
          <div
            class="module-card__icon"
            :style="{ backgroundColor: mod.color + '15', color: mod.color }"
          >
            <span class="material-symbols-outlined">{{ mod.icon }}</span>
          </div>
          <div class="module-card__text">
            <p class="module-card__label">{{ mod.label }}</p>
            <p class="module-card__desc">{{ mod.description }}</p>
          </div>
          <span class="material-symbols-outlined module-card__arrow">chevron_right</span>
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
/* Layout */
.biblio {
  padding: var(--container-padding);
  max-width: 1120px;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .biblio {
    padding: var(--container-padding-sm);
  }
}

/* Header */
.biblio__header {
  display: flex;
  flex-direction: row;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.biblio__title-link {
  background: none;
  border: none;
  cursor: pointer;
  font: inherit;
  color: var(--color-on-surface);
  padding: 0;
}

.biblio__title-link:hover {
  color: var(--color-primary, #4f46e5);
}

.biblio__subtitle {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  margin-top: var(--space-1);
}

/* Progress Bar (replaces Refresh button during normalization) */
.biblio__progress-wrap {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: var(--space-1);
  flex-shrink: 0;
}

.biblio__progress-icon {
  font-size: 18px;
  color: var(--color-primary, #4f46e5);
}

.biblio__progress-text {
  font-size: 12px;
  color: var(--color-on-surface-variant);
  font-weight: var(--font-weight-semibold);
}

.biblio__progress-bar {
  width: 120px;
  height: 4px;
  background: var(--color-border);
  border-radius: 2px;
  overflow: hidden;
}

.biblio__progress-fill {
  height: 100%;
  width: 40%;
  background: var(--color-primary, #4f46e5);
  border-radius: 2px;
  animation: biblio-progress-slide 1.5s ease-in-out infinite;
}

@keyframes biblio-progress-slide {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(350%);
  }
}

/* Fade transition for Refresh ↔ Progress bar */
.biblio-fade-enter-active,
.biblio-fade-leave-active {
  transition: opacity 0.3s ease;
}
.biblio-fade-enter-from,
.biblio-fade-leave-to {
  opacity: 0;
}

/* Refresh Button */
.biblio__refresh-btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-4);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: #ffffff;
  color: var(--color-on-surface);
  font-size: var(--font-size-body);
  font-family: inherit;
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  white-space: nowrap;
  transition:
    border-color 0.2s,
    box-shadow 0.2s,
    background 0.2s,
    transform 0.1s ease;
  flex-shrink: 0;
}

.biblio__refresh-btn:hover:not(:disabled) {
  border-color: #818cf8;
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.12);
}

.biblio__refresh-btn:active:not(:disabled) {
  transform: scale(0.95);
  background: #f5f3ff;
}

.biblio__refresh-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.biblio__refresh-btn .material-symbols-outlined {
  font-size: 18px;
}

.biblio__spin {
  animation: biblio-spin 1s linear infinite;
}

@keyframes biblio-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* KPI Row */
.biblio__kpis {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--space-3);
  margin-bottom: var(--space-8);
}

@media (min-width: 768px) {
  .biblio__kpis {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (min-width: 1024px) {
  .biblio__kpis {
    grid-template-columns: repeat(6, 1fr);
  }
}

.kpi-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  box-shadow: var(--shadow-sm);
}

.kpi-card__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.kpi-card__icon {
  width: 36px;
  height: 36px;
  border-radius: var(--radius-default);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  flex-shrink: 0;
}

.kpi-card__icon--blue {
  background-color: rgba(59, 130, 246, 0.1);
  color: #3b82f6;
}

.kpi-card__icon--purple {
  background-color: rgba(139, 92, 246, 0.1);
  color: #8b5cf6;
}

.kpi-card__icon--teal {
  background-color: rgba(20, 184, 166, 0.1);
  color: #14b8a6;
}

.kpi-card__icon--amber {
  background-color: rgba(245, 158, 11, 0.1);
  color: #f59e0b;
}

.kpi-card__icon--pink {
  background-color: rgba(236, 72, 153, 0.1);
  color: #ec4899;
}

.kpi-card__icon--green {
  background-color: rgba(16, 185, 129, 0.1);
  color: #10b981;
}

.kpi-card__value {
  font-size: 24px;
  font-weight: 800;
  color: var(--color-on-surface);
  line-height: 1;
}

.kpi-card__spinner {
  font-size: 22px;
  color: var(--color-outline);
  line-height: 1;
}

.kpi-card__label {
  font-size: 12px;
  color: var(--color-outline);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.03em;
  margin-top: auto;
}

.kpi-card__label--center {
  text-align: center;
  width: 100%;
}

/* Analysis Modules */
.biblio__section-label {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  color: var(--color-on-surface-variant);
  margin-bottom: var(--space-4);
}

.biblio__module-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: var(--space-3);
}

@media (min-width: 768px) {
  .biblio__module-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (min-width: 1024px) {
  .biblio__module-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}

.module-card {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  width: 100%;
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-4) var(--space-5);
  box-shadow: var(--shadow-sm);
  cursor: pointer;
  text-align: left;
  font-family: inherit;
  font-size: inherit;
  color: inherit;
  transition:
    border-color 0.2s,
    box-shadow 0.2s;
}

.module-card:hover {
  border-color: #818cf8;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.15);
}

.module-card__icon {
  width: 44px;
  height: 44px;
  border-radius: var(--radius-default);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  flex-shrink: 0;
}

.module-card__text {
  flex: 1;
  min-width: 0;
}

.module-card__label {
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  font-size: var(--font-size-body);
}

.module-card__desc {
  color: var(--color-outline);
  font-size: 12px;
  margin-top: 2px;
}

.module-card__arrow {
  color: var(--color-outline);
  font-size: 18px;
  flex-shrink: 0;
  transition: transform 0.2s;
}

.module-card:hover .module-card__arrow {
  transform: translateX(2px);
  color: #6366f1;
}

/* Inline Sparkline inside PUBS/YEAR KPI card */
.kpi-card--chart {
  position: relative;
}

.kpi-sparkline {
  margin-top: var(--space-1);
  width: 100%;
}

.kpi-sparkline__bars {
  display: flex;
  align-items: flex-end;
  gap: 2px;
  height: 48px;
  width: 100%;
}

.kpi-sparkline__bar-wrap {
  flex: 1;
  min-width: 0;
  height: 100%;
  display: flex;
  align-items: flex-end;
  position: relative;
}

.kpi-sparkline__bar {
  width: 100%;
  min-height: 2px;
  background: linear-gradient(to top, #fbbf24, #f59e0b);
  border-radius: 2px 2px 0 0;
  transition:
    height 0.3s ease,
    opacity 0.15s ease;
  cursor: pointer;
  position: relative;
}

.kpi-sparkline__bar--blue {
  background: linear-gradient(to top, #60a5fa, #3b82f6);
}

.kpi-sparkline__bar--purple {
  background: linear-gradient(to top, #a78bfa, #8b5cf6);
}

.kpi-sparkline__bar--green {
  background: linear-gradient(to top, #34d399, #10b981);
}

.kpi-sparkline__bar:hover {
  opacity: 0.8;
}

.kpi-sparkline__tooltip {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
  background: var(--color-on-surface, #1e293b);
  color: #ffffff;
  font-size: 11px;
  font-weight: var(--font-weight-semibold);
  padding: 3px 8px;
  border-radius: var(--radius-default, 4px);
  white-space: nowrap;
  pointer-events: none;
  opacity: 0;
  transition: opacity 0.15s ease;
  z-index: 2;
}

.kpi-sparkline__bar:hover .kpi-sparkline__tooltip {
  opacity: 1;
}

.kpi-sparkline__years {
  display: flex;
  justify-content: space-between;
  margin-top: 2px;
  font-size: 10px;
  color: var(--color-outline);
  line-height: 1;
}

/* Modal Overlay */
.biblio-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}

.biblio-modal {
  background: #ffffff;
  border-radius: var(--radius-md);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
  padding: var(--space-8) var(--space-10);
  max-width: 400px;
  width: 90%;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-4);
}

.biblio-modal__icon {
  font-size: 40px;
  color: var(--color-primary, #4f46e5);
}

.biblio-modal__title {
  font-size: 18px;
  font-weight: 700;
  color: var(--color-on-surface);
  margin: 0;
}

.biblio-modal__message {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  line-height: 1.5;
  margin: 0;
}

.biblio-modal__btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-2) var(--space-8);
  background: var(--color-primary, #4f46e5);
  color: #ffffff;
  border: none;
  border-radius: var(--radius-md);
  font-size: var(--font-size-body);
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: opacity 0.2s;
  margin-top: var(--space-2);
}

.biblio-modal__btn:hover {
  opacity: 0.9;
}
</style>
