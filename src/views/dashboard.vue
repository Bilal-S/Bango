<script setup lang="ts">
import { onMounted, ref, nextTick } from 'vue';
import { useRouter } from 'vue-router';
import {
  useDashboard,
  formatAuditAction,
  formatRelativeTimeParts,
} from '@/composables/use-dashboard';
import { useDemo } from '@/composables/use-demo';
import { useExport } from '@/composables/use-export';

const router = useRouter();
const {
  counts,
  totalNonDuplicate,
  screenedByAi,
  screenedByUser,
  groupedAudit,
  loading,
  loadingMoreActivities,
  hasMoreActivities,
  error,
  hasArticles,
  screeningPercentage,
  // Dynamic primary CTA (Connect LLM / Start AI Screening / Build Wiki / Review Wiki)
  cta,
  refresh,
  loadMoreActivities,
} = useDashboard();

// Activity list scroll container ref (for preserving scroll position on load-more)
const activityListEl = ref<HTMLElement | null>(null);

// Track batch boundaries so we can render a thicker divider between "more" batches.
// batchBoundaryIndices holds the index of the first item in each batch after the first.
const batchBoundaryIndices = ref<Set<number>>(new Set());

// Re-fetch data every time dashboard is mounted (e.g. after import + invalidation)
onMounted(() => {
  refresh();
});

/**
 * Load more activities while preserving the scroll position so the user sees
 * the newly appended records. Without this, the browser keeps scrollTop fixed
 * and the new content appears below the fold (user has to scroll down manually).
 */
async function handleLoadMore(): Promise<void> {
  const el = activityListEl.value;
  const prevScrollHeight = el?.scrollHeight ?? 0;
  const prevCount = groupedAudit.value.length;
  await loadMoreActivities();
  // Record the batch boundary so the template can render a thicker divider
  // before the first newly appended item.
  if (groupedAudit.value.length > prevCount) {
    batchBoundaryIndices.value.add(prevCount);
  }
  await nextTick();
  if (el) {
    // Scroll to where the new content starts so the user sees the appended records
    el.scrollTop = prevScrollHeight;
  }
}

/** Check if an item at the given index is the first in a new "more" batch. */
function isBatchBoundary(index: number): boolean {
  return batchBoundaryIndices.value.has(index);
}

interface StatusTile {
  key: 'duplicate' | 'working' | 'included' | 'rejected';
  label: string;
  icon: string;
  description: string;
  cssClass: string;
}

const statusTiles: StatusTile[] = [
  {
    key: 'duplicate',
    label: 'Duplicates',
    icon: 'content_copy',
    description: 'Resolved duplicates',
    cssClass: 'status-tile--duplicate',
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
    label: 'Import Articles',
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

function navigateToArticlesWithStatus(status: string): void {
  router.push({ path: '/articles', query: { status } });
}

/** Navigate to a specific article in the All articles view. */
function navigateToArticle(articleId: string): void {
  router.push({ path: '/articles', query: { articleId } });
}

const { demoLoading, demoError, loadDemo } = useDemo(router);
const { importProject } = useExport();

// --- Load Existing Project (from a .bango.json backup file) ---
// Uses a hidden HTML <input type="file"> rather than the Tauri fs dialog so
// the picker can read from any directory (the `fs:allow-read-file` capability
// is scoped to `$DOCUMENT/**`). This mirrors the Settings import-backup flow.
const projectFileInput = ref<HTMLInputElement | null>(null);
const projectLoading = ref(false);
const projectError = ref<string | null>(null);

/** Trigger the hidden file input click (opens the OS file picker). */
function loadExistingProject(): void {
  if (projectLoading.value) return;
  projectError.value = null;
  projectFileInput.value?.click();
}

/** Handle the file chosen via the hidden input: import via the shared
 *  `useExport().importProject` helper (which handles the
 *  `import_project_backup` IPC + full store refresh + loading overlay). */
async function onProjectFileSelected(event: Event): Promise<void> {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0];
  if (!file) return;
  // Reset the input value so selecting the same file twice re-fires change.
  target.value = '';
  projectLoading.value = true;
  projectError.value = null;
  try {
    const ok = await importProject(file);
    if (ok) router.push('/');
  } catch (e: unknown) {
    projectError.value = e instanceof Error ? e.message : String(e);
  } finally {
    projectLoading.value = false;
  }
}
</script>

<template>
  <div class="dashboard">
    <!-- Page Header -->
    <section class="dashboard__header">
      <div class="dashboard__header-text">
        <h1 class="page-title">Project Dashboard</h1>
        <p class="dashboard__subtitle"><b>Bango - Your Literature Review Assistant</b></p>
      </div>
      <button v-if="hasArticles" class="dashboard__cta" @click="navigateTo(cta.route)">
        <span class="material-symbols-outlined dashboard__cta-icon">{{ cta.icon }}</span>
        {{ cta.label }}
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
            Start a new systematic review or load an existing Bango project.
          </p>

          <!-- Section 1: Start a New Project -->
          <div class="dashboard__empty-section">
            <h3 class="dashboard__empty-section-label">Start a New Project</h3>
            <button class="dashboard__empty-cta" @click="navigateTo('/criteria')">
              <span class="material-symbols-outlined dashboard__empty-cta-icon">rule</span>
              Set Criteria
            </button>
            <button class="dashboard__empty-cta" @click="navigateTo('/import')">
              <span class="material-symbols-outlined dashboard__empty-cta-icon">upload_file</span>
              Import Articles
            </button>
          </div>

          <!-- Divider between the two sections -->
          <div class="dashboard__empty-divider" aria-hidden="true">
            <span>or load an existing project</span>
          </div>

          <!-- Section 2: Load Existing Project -->
          <div class="dashboard__empty-section">
            <h3 class="dashboard__empty-section-label">Load Existing Project</h3>
            <button
              class="dashboard__empty-cta"
              :disabled="projectLoading"
              @click="loadExistingProject()"
            >
              <span
                v-if="projectLoading"
                class="material-symbols-outlined dashboard__empty-cta-icon"
                >progress_activity</span
              >
              <span v-else class="material-symbols-outlined dashboard__empty-cta-icon"
                >folder_open</span
              >
              {{ projectLoading ? 'Loading…' : 'Load from File' }}
            </button>
            <span class="dashboard__empty-or">or</span>
            <button
              class="dashboard__empty-cta dashboard__empty-cta--secondary"
              :disabled="demoLoading"
              @click="loadDemo()"
            >
              <span v-if="demoLoading" class="material-symbols-outlined dashboard__empty-cta-icon"
                >progress_activity</span
              >
              <span v-else class="material-symbols-outlined dashboard__empty-cta-icon"
                >science</span
              >
              {{ demoLoading ? 'Loading…' : 'Load Demo Project' }}
            </button>
          </div>

          <p v-if="projectError" class="dashboard__empty-error">{{ projectError }}</p>
          <p v-if="demoError" class="dashboard__empty-error">{{ demoError }}</p>

          <!-- Hidden file input backing the "Load from File" button.
               Uses the HTML picker (not the Tauri fs dialog) so files can be
               read from any directory; mirrors Settings > Import Backup. -->
          <input
            ref="projectFileInput"
            type="file"
            accept=".bango.json,.json"
            class="dashboard__hidden-input"
            @change="onProjectFileSelected"
          />
        </div>
      </section>

      <template v-else>
        <!-- Status Count Tiles -->
        <section class="dashboard__stats">
          <button
            v-for="tile in statusTiles"
            :key="tile.key"
            class="status-tile"
            :class="tile.cssClass"
            @click="navigateToArticlesWithStatus(tile.key)"
          >
            <div class="status-tile__top">
              <span class="material-symbols-outlined status-tile__icon">{{ tile.icon }}</span>
              <span class="status-tile__badge">{{ tile.label }}</span>
            </div>
            <div class="status-tile__value">
              {{ counts[tile.key].toLocaleString() }}
            </div>
            <p class="status-tile__desc">{{ tile.description }}</p>
          </button>
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
              <div v-if="groupedAudit.length === 0" class="dashboard__no-activity">
                <p>No recent activity to display.</p>
              </div>
              <div v-else ref="activityListEl" class="dashboard__activity-list">
                <template v-for="(entry, index) in groupedAudit" :key="entry.id">
                  <hr v-if="isBatchBoundary(index)" class="activity-item__batch-divider" />
                  <div
                    class="activity-item"
                    :class="{ 'activity-item--clickable': entry.articleId }"
                  >
                    <button
                      v-if="entry.articleId"
                      class="activity-item__dot"
                      :class="{
                        'activity-item__dot--ai': entry.source === 'ai',
                        'activity-item__dot--system': entry.source === 'system',
                      }"
                      title="Go to article"
                      @click="navigateToArticle(entry.articleId)"
                    >
                      <span class="material-symbols-outlined activity-item__dot-icon">{{
                        entry.source === 'ai'
                          ? 'auto_awesome'
                          : entry.source === 'system'
                            ? 'settings'
                            : 'radio_button_checked'
                      }}</span>
                    </button>
                    <div
                      v-else
                      class="activity-item__dot"
                      :class="{
                        'activity-item__dot--ai': entry.source === 'ai',
                        'activity-item__dot--system': entry.source === 'system',
                      }"
                    >
                      <span class="material-symbols-outlined activity-item__dot-icon">{{
                        entry.source === 'ai'
                          ? 'auto_awesome'
                          : entry.source === 'system'
                            ? 'settings'
                            : 'radio_button_checked'
                      }}</span>
                    </div>
                    <div class="activity-item__content">
                      <p class="activity-item__text">
                        <span class="activity-item__action">
                          {{ formatAuditAction(entry.action) }}
                        </span>
                        <span v-if="entry.source === 'ai'" class="activity-item__source">
                          by AI</span
                        >
                        <span v-if="entry.articleTitle" class="activity-item__title">
                          - {{ entry.articleTitle
                          }}{{ entry.articleTitle.length >= 55 ? '...' : '' }}
                        </span>
                        <span v-if="entry.count && entry.count > 1" class="activity-item__count">
                          {{ entry.count }} articles
                        </span>
                      </p>
                      <p v-if="entry.details" class="activity-item__details">{{ entry.details }}</p>
                    </div>
                    <div class="activity-item__time-col">
                      <span class="activity-item__time-value">{{
                        formatRelativeTimeParts(entry.timestamp).value
                      }}</span>
                      <span class="activity-item__time-suffix">{{
                        formatRelativeTimeParts(entry.timestamp).suffix
                      }}</span>
                    </div>
                  </div>
                </template>

                <!-- Load more link (click-based pagination) -->
                <button
                  v-if="hasMoreActivities"
                  class="dashboard__load-more"
                  :disabled="loadingMoreActivities"
                  @click="handleLoadMore"
                >
                  <span
                    v-if="loadingMoreActivities"
                    class="material-symbols-outlined dashboard__scroll-spinner"
                    >progress_activity</span
                  >
                  <span v-else>more</span>
                </button>
              </div>
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
                  {{ totalNonDuplicate.toLocaleString() }}
                </span>
              </div>
              <div class="dashboard__summary-row">
                <span class="dashboard__summary-label">Screened by AI</span>
                <span class="dashboard__summary-value">
                  {{ screenedByAi.toLocaleString() }}
                </span>
              </div>
              <div class="dashboard__summary-row">
                <span class="dashboard__summary-label">Screened by User</span>
                <span class="dashboard__summary-value">
                  {{ screenedByUser.toLocaleString() }}
                </span>
              </div>
              <div class="dashboard__progress">
                <div class="dashboard__progress-header">
                  <span class="dashboard__progress-label">Screening Progress</span>
                  <span class="dashboard__progress-pct">{{ screeningPercentage }}%</span>
                </div>
                <div class="dashboard__progress-track">
                  <div
                    class="dashboard__progress-fill"
                    :style="{ width: screeningPercentage + '%' }"
                  />
                </div>
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

.dashboard__empty-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  align-items: center;
}

.dashboard__empty-section-label {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  color: var(--color-on-surface-variant);
  margin-bottom: var(--space-1);
}

/* Divider between the two empty-state sections. A hairline rule with a
   caption in the gap, so the two sections read as distinct groups. */
.dashboard__empty-divider {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin: var(--space-6) 0;
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.dashboard__empty-divider::before,
.dashboard__empty-divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background-color: var(--color-border);
}

.dashboard__empty-actions {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  align-items: center;
}

.dashboard__empty-or {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  text-transform: lowercase;
}

.dashboard__empty-cta {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  width: 100%;
  background-color: var(--color-primary-container);
  color: var(--color-on-primary);
  padding: 10px 20px;
  border: none;
  border-radius: var(--radius-default);
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  white-space: nowrap;
  font-family: inherit;
}

.dashboard__empty-cta:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.dashboard__empty-cta-icon {
  font-size: 18px;
}

.dashboard__empty-cta--secondary {
  background-color: #ffffff;
  color: #4f46e5;
  border: 1px solid #c7d2fe;
}

.dashboard__empty-cta--secondary:hover:not(:disabled) {
  background-color: #eef2ff;
}

.dashboard__empty-cta--secondary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.dashboard__empty-error {
  color: var(--color-error, #dc2626);
  font-size: var(--font-size-caption);
  margin-top: var(--space-3);
}

/* Visually hidden file input backing the "Load from File" button. */
.dashboard__hidden-input {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
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
  display: block;
  width: 100%;
  background-color: #ffffff;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
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

.status-tile:hover {
  border-color: #c7d2fe;
  box-shadow: 0 4px 12px rgba(99, 102, 241, 0.15);
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
.status-tile--duplicate .status-tile__badge {
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
  max-height: 640px;
  overflow-y: auto;
}

.activity-item {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  transition: background-color 0.15s;
}

.activity-item:hover {
  background-color: #f8fafc;
}

.activity-item:not(:first-child) {
  border-top: 1px solid #f1f5f9;
}

.activity-item--clickable {
  cursor: default;
}

.activity-item__dot {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background-color: #f1f5f9;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  border: none;
  padding: 0;
  cursor: default;
  transition:
    background-color 0.15s,
    transform 0.1s;
}

.activity-item__dot--ai {
  background-color: #eef2ff;
  color: #4f46e5;
}

.activity-item__dot--system {
  background-color: #f1f5f9;
  color: var(--color-outline);
}

/* When the dot is a button (clickable article link), make it interactive */
button.activity-item__dot {
  cursor: pointer;
}

button.activity-item__dot:hover {
  background-color: #c7d2fe;
  color: #4338ca;
  transform: scale(1.15);
}

.activity-item__dot-icon {
  font-size: 13px;
}

.activity-item__content {
  flex: 1;
  min-width: 0;
}

.activity-item__text {
  font-size: 13px;
  color: var(--color-on-surface);
  line-height: 1.4;
  margin: 0;
}

.activity-item__action {
  font-weight: var(--font-weight-semibold);
}

.activity-item__count {
  font-weight: var(--font-weight-semibold);
  color: #4f46e5;
  margin-left: var(--space-1);
  margin-right: var(--space-1);
}

.activity-item__source {
  color: var(--color-on-surface-variant);
  font-size: 12px;
}

.activity-item__title {
  color: var(--color-on-surface-variant);
  font-style: italic;
}

.activity-item__details {
  font-size: 12px;
  color: var(--color-on-surface-variant);
  margin: 2px 0 0 0;
  line-height: 1.3;
}

.activity-item__time-col {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  min-width: 32px;
  padding-left: var(--space-2);
}

.activity-item__time-value {
  font-size: 11px;
  font-weight: var(--font-weight-semibold);
  color: var(--color-outline);
  line-height: 1.1;
}

.activity-item__time-suffix {
  font-size: 10px;
  color: var(--color-outline);
  line-height: 1.1;
}

/* Thicker divider between "more" batches */
.activity-item__batch-divider {
  border: none;
  border-top: 2px solid #cbd5e1;
  margin: 0;
  width: 100%;
}

/* Load more link (click-based pagination) */
.dashboard__load-more {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
  border: none;
  border-top: 1px solid #f1f5f9;
  background: transparent;
  color: #4f46e5;
  font-size: 12px;
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition: background-color 0.15s;
  font-family: inherit;
}

.dashboard__load-more:hover:not(:disabled) {
  background-color: #eef2ff;
}

.dashboard__load-more:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.dashboard__scroll-spinner {
  font-size: 14px;
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
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

.dashboard__progress {
  margin-top: var(--space-3);
  padding-top: var(--space-3);
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.dashboard__progress-header {
  display: flex;
  justify-content: space-between;
  margin-bottom: var(--space-2);
}

.dashboard__progress-label {
  font-size: var(--font-size-caption);
  color: #94a3b8;
}

.dashboard__progress-pct {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: #ffffff;
  font-family: var(--font-family-mono);
}

.dashboard__progress-track {
  width: 100%;
  height: 6px;
  background-color: rgba(255, 255, 255, 0.15);
  border-radius: 3px;
  overflow: hidden;
}

.dashboard__progress-fill {
  height: 100%;
  background-color: #818cf8;
  border-radius: 3px;
  transition: width 0.4s ease;
}
</style>
