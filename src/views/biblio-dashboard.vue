<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { useBibliometrics } from '../composables/use-bibliometrics';

const router = useRouter();

const { kpis, refreshing, refresh } = useBibliometrics();

const includedCount = computed(() => kpis.value.includedCount);
const totalCitations = computed(() => kpis.value.totalCitations);
const uniqueAuthors = computed(() => kpis.value.uniqueAuthors);
const avgYear = computed(() => (kpis.value.avgYear !== null ? kpis.value.avgYear.toFixed(1) : '—'));
const avgCitationsPerArticle = computed(() =>
  kpis.value.includedCount > 0
    ? (kpis.value.totalCitations / kpis.value.includedCount).toFixed(1)
    : '—'
);
const growthRate = computed(() =>
  kpis.value.growthRate !== null
    ? `${kpis.value.growthRate >= 0 ? '+' : ''}${kpis.value.growthRate.toFixed(1)}%`
    : '—'
);
const yearFrom = computed(() => kpis.value.yearFrom ?? '—');
const yearTo = computed(() => kpis.value.yearTo ?? '—');

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
</script>

<template>
  <div class="biblio">
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
      <button class="biblio__refresh-btn" :disabled="refreshing" @click="refresh">
        <span class="material-symbols-outlined" :class="{ biblio__spin: refreshing }">
          {{ refreshing ? 'progress_activity' : 'sync' }}
        </span>
        {{ refreshing ? 'Normalizing…' : 'Refresh' }}
      </button>
    </section>

    <!-- KPI Row — compact horizontal layout from High-Contrast Research Hub -->
    <section class="biblio__kpis">
      <div class="kpi-card">
        <div class="kpi-card__icon kpi-card__icon--blue">
          <span class="material-symbols-outlined">description</span>
        </div>
        <span class="kpi-card__value">{{ includedCount.toLocaleString() }}</span>
        <span class="kpi-card__label">Included Articles</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__icon kpi-card__icon--purple">
          <span class="material-symbols-outlined">format_quote</span>
        </div>
        <span class="kpi-card__value">{{ totalCitations.toLocaleString() }}</span>
        <span class="kpi-card__label">Total Citations</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__icon kpi-card__icon--teal">
          <span class="material-symbols-outlined">group</span>
        </div>
        <span class="kpi-card__value">{{ uniqueAuthors.toLocaleString() }}</span>
        <span class="kpi-card__label">Unique Authors</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__icon kpi-card__icon--amber">
          <span class="material-symbols-outlined">calendar_month</span>
        </div>
        <span class="kpi-card__value">{{ avgYear }}</span>
        <span class="kpi-card__label">Avg Publication Year</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__icon kpi-card__icon--pink">
          <span class="material-symbols-outlined">star</span>
        </div>
        <span class="kpi-card__value">{{ avgCitationsPerArticle }}</span>
        <span class="kpi-card__label">Avg Citations / Article</span>
      </div>
      <div class="kpi-card">
        <div class="kpi-card__icon kpi-card__icon--green">
          <span class="material-symbols-outlined">trending_up</span>
        </div>
        <span class="kpi-card__value">{{ growthRate }}</span>
        <span class="kpi-card__label">Growth Rate</span>
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
    background 0.2s;
  flex-shrink: 0;
}

.biblio__refresh-btn:hover:not(:disabled) {
  border-color: #818cf8;
  box-shadow: 0 2px 8px rgba(99, 102, 241, 0.12);
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

.kpi-card__label {
  font-size: 12px;
  color: var(--color-outline);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.03em;
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
</style>
