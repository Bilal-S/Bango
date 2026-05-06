<script setup lang="ts">
import { useRouter } from 'vue-router';
import { useDashboard, formatAuditAction, formatRelativeTime } from '@/composables/use-dashboard';

const router = useRouter();
const { counts, screeningProgress, recentAudit, loading, error, hasArticles, refresh } =
  useDashboard();

interface StatusTile {
  key: 'imported' | 'working' | 'included' | 'rejected';
  label: string;
  icon: string;
  description: string;
  cssClass: string;
}

const statusTiles: StatusTile[] = [
  {
    key: 'imported',
    label: 'Imported',
    icon: 'import_export',
    description: 'Total citations in library',
    cssClass: 'status-tile--imported',
  },
  {
    key: 'working',
    label: 'Working',
    icon: 'pending',
    description: 'Awaiting title/abstract review',
    cssClass: 'status-tile--working',
  },
  {
    key: 'included',
    label: 'Included',
    icon: 'check_circle',
    description: 'Confirmed for full-text analysis',
    cssClass: 'status-tile--included',
  },
  {
    key: 'rejected',
    label: 'Rejected',
    icon: 'cancel',
    description: 'Does not meet inclusion criteria',
    cssClass: 'status-tile--rejected',
  },
];

interface QuickAction {
  label: string;
  description: string;
  icon: string;
  route: string;
}

const quickActions: QuickAction[] = [
  {
    label: 'Import RIS/BibTeX',
    description: 'Add new references from search',
    icon: 'upload_file',
    route: '/import',
  },
  {
    label: 'Edit Research Criteria',
    description: 'Update inclusion & exclusion rules',
    icon: 'rule',
    route: '/criteria',
  },
  {
    label: 'View PRISMA Flow Diagram',
    description: 'Track reporting and transparency',
    icon: 'account_tree',
    route: '/prisma',
  },
];

function navigateTo(route: string): void {
  router.push(route);
}
</script>

<template>
  <div class="dashboard">
    <!-- Page Header -->
    <section class="dashboard__header">
      <div class="dashboard__header-text">
        <span class="dashboard__badge">Active Project</span>
        <h1 class="page-title">Project Dashboard</h1>
        <p class="dashboard__subtitle">AI-assisted systematic literature review</p>
      </div>
      <button v-if="hasArticles" class="dashboard__cta" @click="navigateTo('/screening')">
        <span class="material-symbols-outlined dashboard__cta-icon">play_arrow</span>
        Start AI Screening
      </button>
    </section>

    <!-- Loading State -->
    <div v-if="loading" class="dashboard__loading">
      <p>Loading project data...</p>
    </div>

    <!-- Error State -->
    <div v-else-if="error" class="dashboard__error">
      <p>Failed to load dashboard data.</p>
      <p class="dashboard__error-detail">{{ error }}</p>
      <button class="dashboard__retry-btn" @click="refresh">Retry</button>
    </div>

    <template v-else>
      <!-- Empty State -->
      <section v-if="!hasArticles" class="dashboard__empty">
        <div class="dashboard__empty-card">
          <div class="dashboard__empty-icon material-symbols-outlined">import_export</div>
          <h2 class="dashboard__empty-title">No articles yet</h2>
          <p class="dashboard__empty-desc">
            Import an RIS or BibTeX file to get started with your systematic review.
          </p>
          <button class="dashboard__empty-cta" @click="navigateTo('/import')">
            Import References
          </button>
        </div>
      </section>

      <template v-else>
        <!-- Status Count Tiles -->
        <section class="dashboard__stats">
          <div
            v-for="tile in statusTiles"
            :key="tile.key"
            class="status-tile"
            :class="tile.cssClass"
          >
            <div class="status-tile__top">
              <span class="material-symbols-outlined status-tile__icon">{{ tile.icon }}</span>
              <span class="status-tile__badge">{{ tile.label }}</span>
            </div>
            <div class="status-tile__value">
              {{ counts[tile.key].toLocaleString() }}
            </div>
            <p class="status-tile__desc">{{ tile.description }}</p>
          </div>
        </section>

        <!-- Main Content Grid -->
        <div class="dashboard__grid">
          <!-- Left Column: Activity + Progress -->
          <div class="dashboard__main">
            <!-- Recent Activity -->
            <div class="dashboard__card">
              <div class="dashboard__card-header">
                <h3 class="dashboard__card-title">Recent Activity</h3>
              </div>
              <div v-if="recentAudit.length === 0" class="dashboard__no-activity">
                <p>No recent activity to display.</p>
              </div>
              <div v-else class="dashboard__activity-list">
                <div v-for="entry in recentAudit" :key="entry.id" class="activity-item">
                  <div
                    class="activity-item__icon"
                    :class="{
                      'activity-item__icon--ai': entry.source === 'ai',
                      'activity-item__icon--system': entry.source === 'system',
                    }"
                  >
                    <template v-if="entry.source === 'ai'"
                      ><span class="material-symbols-outlined">auto_awesome</span></template
                    >
                    <template v-else-if="entry.source === 'system'"
                      ><span class="material-symbols-outlined">settings</span></template
                    >
                    <template v-else
                      ><span class="material-symbols-outlined">radio_button_checked</span></template
                    >
                  </div>
                  <div class="activity-item__content">
                    <p class="activity-item__text">
                      <span class="activity-item__action">
                        {{ formatAuditAction(entry.action) }}
                      </span>
                      <span v-if="entry.details" class="activity-item__details">
                        — {{ entry.details }}
                      </span>
                    </p>
                    <p class="activity-item__time">
                      {{ formatRelativeTime(entry.timestamp) }}
                    </p>
                  </div>
                </div>
              </div>
            </div>

            <!-- Screening Progress -->
            <div v-if="screeningProgress.total > 0" class="dashboard__card">
              <div class="dashboard__card-header">
                <h3 class="dashboard__card-title">Screening Progress</h3>
                <span class="dashboard__progress-pct"> {{ screeningProgress.percentage }}% </span>
              </div>
              <div class="dashboard__progress-track">
                <div
                  class="dashboard__progress-fill"
                  :style="{ width: screeningProgress.percentage + '%' }"
                ></div>
              </div>
              <p class="dashboard__progress-text">
                {{ screeningProgress.screened.toLocaleString() }} of
                {{ screeningProgress.total.toLocaleString() }} articles screened
              </p>
            </div>
          </div>

          <!-- Right Column: Quick Actions -->
          <div class="dashboard__sidebar">
            <h4 class="dashboard__sidebar-label">Quick Actions</h4>
            <button
              v-for="action in quickActions"
              :key="action.route"
              class="quick-action"
              @click="navigateTo(action.route)"
            >
              <div class="quick-action__icon material-symbols-outlined">
                {{ action.icon }}
              </div>
              <div class="quick-action__text">
                <p class="quick-action__label">{{ action.label }}</p>
                <p class="quick-action__desc">{{ action.description }}</p>
              </div>
            </button>

            <!-- Summary Stats Card -->
            <div class="dashboard__summary-card">
              <h4 class="dashboard__summary-title">Project Summary</h4>
              <div class="dashboard__summary-row">
                <span class="dashboard__summary-label">Total Articles</span>
                <span class="dashboard__summary-value">
                  {{ counts.total.toLocaleString() }}
                </span>
              </div>
              <div class="dashboard__summary-row">
                <span class="dashboard__summary-label">Screened</span>
                <span class="dashboard__summary-value">
                  {{ screeningProgress.screened.toLocaleString() }}
                </span>
              </div>
              <div class="dashboard__summary-row">
                <span class="dashboard__summary-label">Completion</span>
                <span class="dashboard__summary-value"> {{ screeningProgress.percentage }}% </span>
              </div>
            </div>
          </div>
        </div>
      </template>
    </template>
  </div>
</template>

<style scoped>
/* Layout */
.dashboard {
  padding: var(--container-padding);
  max-width: 1120px;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .dashboard {
    padding: var(--container-padding-sm);
  }
}

.dashboard__header {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-bottom: var(--space-8);
}

@media (min-width: 768px) {
  .dashboard__header {
    flex-direction: row;
    justify-content: space-between;
    align-items: flex-end;
  }
}

.dashboard__badge {
  display: inline-block;
  background-color: #eef2ff;
  color: #4f46e5;
  padding: 2px 8px;
  border-radius: var(--radius-sm);
  font-size: 10px;
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-bottom: var(--space-2);
}

.dashboard__title {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-display);
  color: var(--color-on-surface);
}

.dashboard__subtitle {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  margin-top: var(--space-1);
}

.dashboard__cta {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  background-color: var(--color-primary-container);
  color: var(--color-on-primary);
  padding: 10px 24px;
  border: none;
  border-radius: var(--radius-md);
  font-size: var(--font-size-h2);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  white-space: nowrap;
  transition: box-shadow 0.2s;
}

.dashboard__cta:hover {
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
}

.dashboard__cta:active {
  transform: scale(0.98);
}

.dashboard__cta-icon {
  font-size: 18px;
}

/* Loading & Error States */
.dashboard__loading,
.dashboard__error {
  padding: var(--space-10) var(--space-6);
  text-align: center;
  color: var(--color-on-surface-variant);
}

.dashboard__error-detail {
  font-size: var(--font-size-caption);
  color: var(--color-outline);
  margin-top: var(--space-2);
}

.dashboard__retry-btn {
  margin-top: var(--space-4);
  padding: var(--space-2) var(--space-4);
  background-color: var(--color-primary-container);
  color: var(--color-on-primary);
  border: none;
  border-radius: var(--radius-default);
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

/* Empty State */
.dashboard__empty {
  display: flex;
  justify-content: center;
  padding: var(--space-10) 0;
}

.dashboard__empty-card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-10) var(--space-8);
  text-align: center;
  max-width: 400px;
}

.dashboard__empty-icon {
  width: 48px;
  height: 48px;
  background-color: #eef2ff;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 22px;
  color: #4f46e5;
  margin: 0 auto var(--space-4);
}

.dashboard__empty-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin-bottom: var(--space-2);
}

.dashboard__empty-desc {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
  margin-bottom: var(--space-6);
}

.dashboard__empty-cta {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  background-color: var(--color-primary-container);
  color: var(--color-on-primary);
  padding: 10px 20px;
  border: none;
  border-radius: var(--radius-default);
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

/* Status Tiles */
.dashboard__stats {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: var(--gutter);
  margin-bottom: var(--space-6);
}

@media (min-width: 1024px) {
  .dashboard__stats {
    grid-template-columns: repeat(4, 1fr);
  }
}

.status-tile {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  box-shadow: var(--shadow-sm);
  transition: border-color 0.2s;
}

.status-tile:hover {
  border-color: #c7d2fe;
}

.status-tile__top {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: var(--space-4);
}

.status-tile__icon {
  color: var(--color-outline);
  font-size: 18px;
}

.status-tile__badge {
  padding: 2px 10px;
  border-radius: var(--radius-pill);
  font-size: 11px;
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.02em;
}

.status-tile__value {
  font-size: 28px;
  font-weight: 800;
  color: var(--color-on-surface);
  line-height: 1;
}

.status-tile__desc {
  color: var(--color-outline);
  font-size: 12px;
  margin-top: var(--space-1);
}

/* Status tile color variants */
.status-tile--imported .status-tile__badge {
  background-color: #dbeafe;
  color: #1e40af;
}

.status-tile--working .status-tile__badge {
  background-color: #fef3c7;
  color: #92400e;
}

.status-tile--included .status-tile__badge {
  background-color: #d1fae5;
  color: #065f46;
}

.status-tile--rejected .status-tile__badge {
  background-color: #ffe4e6;
  color: #9f1239;
}

/* Main Content Grid */
.dashboard__grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: var(--gutter);
}

@media (min-width: 1024px) {
  .dashboard__grid {
    grid-template-columns: 2fr 1fr;
  }
}

.dashboard__main {
  display: flex;
  flex-direction: column;
  gap: var(--gutter);
}

/* Card Pattern */
.dashboard__card {
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  overflow: hidden;
}

.dashboard__card-header {
  padding: var(--space-4);
  border-bottom: 1px solid #f1f5f9;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.dashboard__card-title {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
}

.dashboard__no-activity {
  padding: var(--space-6) var(--space-4);
  text-align: center;
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-body);
}

/* Activity Feed */
.dashboard__activity-list {
  display: flex;
  flex-direction: column;
}

.activity-item {
  display: flex;
  align-items: flex-start;
  gap: var(--space-4);
  padding: var(--space-4);
  transition: background-color 0.15s;
}

.activity-item:hover {
  background-color: #f8fafc;
}

.activity-item + .activity-item {
  border-top: 1px solid #f1f5f9;
}

.activity-item__icon {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background-color: #f1f5f9;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  font-size: 14px;
  color: var(--color-outline);
}

.activity-item__icon--ai {
  background-color: #eef2ff;
  color: #4f46e5;
}

.activity-item__icon--system {
  background-color: #f1f5f9;
  color: var(--color-outline);
}

.activity-item__content {
  flex: 1;
  min-width: 0;
}

.activity-item__text {
  font-size: var(--font-size-body);
  color: var(--color-on-surface);
  line-height: var(--line-height-body);
}

.activity-item__action {
  font-weight: var(--font-weight-semibold);
}

.activity-item__details {
  color: var(--color-on-surface-variant);
}

.activity-item__time {
  font-size: 12px;
  color: var(--color-outline);
  margin-top: 2px;
}

/* Screening Progress */
.dashboard__progress-pct {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-primary-container);
}

.dashboard__progress-track {
  height: 12px;
  background-color: #e2e8f0;
  border-radius: var(--radius-pill);
  overflow: hidden;
  margin: var(--space-4);
}

.dashboard__progress-fill {
  height: 100%;
  background-color: var(--color-primary-container);
  border-radius: var(--radius-pill);
  transition: width 0.6s ease;
}

.dashboard__progress-text {
  padding: 0 var(--space-4) var(--space-4);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

/* Sidebar */
.dashboard__sidebar {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.dashboard__sidebar-label {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  color: var(--color-on-surface-variant);
  padding: 0 var(--space-1);
}

/* Quick Action Cards */
.quick-action {
  display: flex;
  align-items: center;
  gap: var(--space-4);
  width: 100%;
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  padding: var(--space-4);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  cursor: pointer;
  text-align: left;
  transition: border-color 0.2s;
  font-family: inherit;
  font-size: inherit;
  color: inherit;
}

.quick-action:hover {
  border-color: #818cf8;
}

.quick-action__icon {
  width: 40px;
  height: 40px;
  background-color: #eef2ff;
  border-radius: var(--radius-default);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 18px;
  color: #4f46e5;
  flex-shrink: 0;
  transition:
    background-color 0.2s,
    color 0.2s;
}

.quick-action:hover .quick-action__icon {
  background-color: #4f46e5;
  color: #ffffff;
}

.quick-action__label {
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  font-size: var(--font-size-body);
}

.quick-action__desc {
  color: var(--color-outline);
  font-size: 12px;
  margin-top: 2px;
}

/* Summary Card */
.dashboard__summary-card {
  background-color: #1e293b;
  color: #ffffff;
  border-radius: var(--radius-md);
  padding: var(--space-5);
  margin-top: var(--space-4);
  box-shadow: var(--shadow-sm);
}

.dashboard__summary-title {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  margin-bottom: var(--space-3);
}

.dashboard__summary-row {
  display: flex;
  justify-content: space-between;
  padding: var(--space-2) 0;
}

.dashboard__summary-row + .dashboard__summary-row {
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.dashboard__summary-label {
  font-size: var(--font-size-caption);
  color: #94a3b8;
}

.dashboard__summary-value {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: #ffffff;
  font-family: var(--font-family-mono);
}
</style>
