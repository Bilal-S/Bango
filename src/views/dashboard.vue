<script setup lang="ts">
import { onMounted, ref, computed, nextTick } from 'vue';
import { useRouter } from 'vue-router';
import {
  useDashboard,
  formatAuditAction,
  formatRelativeTimeParts,
} from '@/composables/use-dashboard';
import { useDemo } from '@/composables/use-demo';
import { useExport } from '@/composables/use-export';
import { useProjectName, PROJECT_NAME_MAX_LEN } from '@/composables/use-project-name';
import { useToast } from '@/composables/use-toast';
import ClearableInput from '@/components/clearable-input.vue';

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

/** Project name: dblclick h1 or pencil -> ClearableInput; Enter/blur commits, Esc cancels. */
const {
  displayName,
  projectName,
  hasCustomName,
  load: loadProjectName,
  save: saveProjectName,
  clear: clearProjectName,
} = useProjectName();

const toast = useToast();

/** Inline edit input shown instead of read-mode h1. */
const isEditingProjectName = ref(false);
/** Editable draft bound to ClearableInput. */
const projectNameDraft = ref('');
/** Save in flight, disables input + shows saving hint. */
const projectNameSaving = ref(false);

/** Seed draft with current custom name (or empty for placeholder). */
function startEditProjectName(): void {
  if (projectNameSaving.value) return;
  projectNameDraft.value = projectName.value ?? '';
  isEditingProjectName.value = true;
}

/** Commit draft. Trim; empty -> clear; changed -> save; unchanged -> no-op.
 *  Errors surface toast + leave edit state intact for retry. */
async function commitProjectName(): Promise<void> {
  if (!isEditingProjectName.value || projectNameSaving.value) return;
  const trimmed = projectNameDraft.value.trim();
  // Unchanged -> exit without a backend call.
  if (trimmed === (projectName.value ?? '')) {
    isEditingProjectName.value = false;
    projectNameDraft.value = '';
    return;
  }
  projectNameSaving.value = true;
  try {
    if (trimmed === '') {
      await clearProjectName();
    } else {
      await saveProjectName(trimmed);
    }
    isEditingProjectName.value = false;
    projectNameDraft.value = '';
  } catch (e: unknown) {
    toast.show(
      `Failed to save project name: ${e instanceof Error ? e.message : String(e)}`,
      'error'
    );
    // Leave edit state intact for retry.
  } finally {
    projectNameSaving.value = false;
  }
}

/** Discard draft and exit edit mode without saving or clearing. */
function cancelEditProjectName(): void {
  if (projectNameSaving.value) return;
  isEditingProjectName.value = false;
  projectNameDraft.value = '';
}

// Activity list scroll container ref (for preserving scroll position on load-more)
const activityListEl = ref<HTMLElement | null>(null);

/* Batch-boundary indices for "N new" separators + TransitionGroup staggered
   fade-in (backend feed is sorted; new entries land at the end). */
const batchBoundaryIndices = ref<Set<number>>(new Set());

/** Index of the first item in the most recently loaded batch (for animation). */
const newBatchStart = computed<number>(() => {
  const sorted = [...batchBoundaryIndices.value].sort((a, b) => b - a);
  if (sorted.length === 0) return -1;
  return sorted[0]!;
});

/* Re-fetch data on every mount (e.g. after import + invalidation). Load
 * project name in parallel (single-row SELECT; safe since dashboard is not
 * keep-alive cached). */
onMounted(() => {
  refresh();
  void loadProjectName();
});

/**
 * Load more activities while preserving scroll position. New entries land at
 * the end of the sorted feed; prevCount identifies the batch boundary.
 */
async function handleLoadMore(): Promise<void> {
  const el = activityListEl.value;
  const prevScrollHeight = el?.scrollHeight ?? 0;
  const prevCount = groupedAudit.value.length;
  await loadMoreActivities();
  if (groupedAudit.value.length > prevCount) {
    batchBoundaryIndices.value.add(prevCount);
  }
  await nextTick();
  if (el) {
    el.scrollTop = prevScrollHeight;
  }
}

function isBatchBoundary(index: number): boolean {
  return batchBoundaryIndices.value.has(index);
}

function staggeredStyle(index: number): Record<string, string> | undefined {
  const start = newBatchStart.value;
  if (start < 0 || index < start) return undefined;
  return { '--enter-delay': `${(index - start) * 0.05}s` };
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
    label: 'Search with OpenAlex',
    description: 'Discover articles from 300M+ scholarly works',
    icon: 'database_search',
    route: '/articles?status=search',
  },
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
];

function navigateTo(route: string): void {
  router.push(route);
}

function navigateToArticlesWithStatus(status: string): void {
  router.push({ path: '/articles', query: { status } });
}

/** Navigate to article in All articles view. */
function navigateToArticle(articleId: string): void {
  router.push({ path: '/articles', query: { articleId } });
}

const { demoLoading, demoError, loadDemo } = useDemo(router);
const { importProject } = useExport();

/** Wrap `loadDemo` so project name refreshes after demo import. Demo backup
 *  has no `project_name`, so backend clears any existing name; this refresh
 *  prevents the stale pre-demo name showing until navigate-away-and-back. */
async function handleLoadDemo(): Promise<void> {
  await loadDemo();
  if (!demoError.value) {
    void loadProjectName();
  }
}

// Start New Project info dialog - surfaces the export → delete → begin-fresh
// workflow for the single-project model.
const showStartNewProjectDialog = ref(false);

/** Navigate to Project Management settings card. `focus` query param scrolls
 *  that card into view on arrival. */
function goToProjectManagement(): void {
  showStartNewProjectDialog.value = false;
  router.push('/settings?focus=project-management');
}

/** Open Help Guide's "Starting Points" anchor. */
function openHelpGuideStartingPoints(): void {
  showStartNewProjectDialog.value = false;
  router.push('/help?tab=guide#starting-points');
}

/* Hidden HTML <input type="file"> (not the Tauri fs dialog) so the picker
   reads from any directory; mirrors Settings import-backup. */
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
    if (ok) {
      /* Reload name after import: backup without project_name clears the
         target (backend contract); same-route push is a no-op so onMounted
         won't refire. */
      void loadProjectName();
      router.push('/');
    }
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
        <!-- Editable project title. Read mode: double-click the h1 OR click the
             pencil icon to enter edit mode. Edit mode: a ClearableInput replaces
             the h1; Enter/blur commits, Escape cancels, the "x" clears the
             draft (an empty commit reverts to the "Project Dashboard" fallback). -->
        <div class="dashboard__title-row">
          <h1
            v-if="!isEditingProjectName"
            class="page-title dashboard__title"
            :class="{ 'dashboard__title--custom': hasCustomName }"
            :title="'Double-click to edit' + (hasCustomName ? '' : ' (set a project name)')"
            @dblclick="startEditProjectName"
          >
            <span class="dashboard__title-text">{{ displayName }}</span>
            <button
              type="button"
              class="dashboard__title-edit-btn"
              title="Edit project name"
              aria-label="Edit project name"
              @click="startEditProjectName"
            >
              <span class="material-symbols-outlined">edit</span>
            </button>
          </h1>
          <ClearableInput
            v-else
            v-model="projectNameDraft"
            :maxlength="PROJECT_NAME_MAX_LEN"
            :autofocus="true"
            :disabled="projectNameSaving"
            placeholder="Project name (leave empty to reset)"
            input-class="dashboard__title-input"
            class="dashboard__title-edit"
            @enter="commitProjectName"
            @blur="commitProjectName"
            @clear="commitProjectName"
            @keydown.escape.prevent="cancelEditProjectName"
          />
        </div>
        <p class="dashboard__subtitle"><b>Bango - Your Literature Review Assistant</b></p>
      </div>
      <div v-if="hasArticles" class="dashboard__header-actions">
        <button
          class="dashboard__start-new-link"
          title="Back up your current project and start a fresh one"
          @click="showStartNewProjectDialog = true"
        >
          <span class="material-symbols-outlined dashboard__start-new-icon">restart_alt</span>
          Start New Project
        </button>
        <button class="dashboard__cta" @click="navigateTo(cta.route)">
          <span class="material-symbols-outlined dashboard__cta-icon">{{ cta.icon }}</span>
          {{ cta.label }}
        </button>
      </div>
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
              @click="handleLoadDemo"
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
                <TransitionGroup name="activity-item" tag="div" class="dashboard__activity-group">
                  <template v-for="(entry, index) in groupedAudit" :key="entry.id">
                    <div v-if="isBatchBoundary(index)" class="activity-item__batch-separator">
                      <span class="activity-item__batch-separator-label">
                        {{ groupedAudit.length - index }} new
                      </span>
                    </div>
                    <div
                      class="activity-item"
                      :class="{ 'activity-item--clickable': entry.articleId }"
                      :style="staggeredStyle(index)"
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
                        <p v-if="entry.details" class="activity-item__details">
                          {{ entry.details }}
                        </p>
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
                </TransitionGroup>

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

    <!-- Start New Project info dialog (shown when a project is loaded).
         Surfaces the export → delete → begin-fresh workflow for Bango's
         single-project model so the user knows how to start over. -->
    <div
      v-if="showStartNewProjectDialog"
      class="dialog-overlay"
      @click.self="showStartNewProjectDialog = false"
    >
      <div class="dialog">
        <h2>Start a New Project</h2>
        <p class="dialog__desc">
          Bango manages <strong>one project at a time</strong>. You need to delete this project to
          start a new one. You do this by using the <strong>Project Management</strong>
          features in the Settings Area.
        </p>
        <div class="dialog__info-box">
          <span class="material-symbols-outlined">info</span>
          <div>
            <p><strong>Recommended workflow:</strong></p>
            <ol class="dashboard__start-new-steps">
              <li>
                <strong>Back up</strong> - Export the current project to a
                <code>.bango.json</code> file so you can restore it later.
              </li>
              <li>
                <strong>Delete</strong> - Use "Delete All Data" in Settings to clear the database.
                This also removes the on-disk Wiki, but does not delete full-text PDFs.
              </li>
              <li>
                <strong>Begin fresh</strong> - Define new criteria, import new articles, or search
                OpenAlex to discover articles for your new review.
              </li>
            </ol>
          </div>
        </div>
        <div class="dialog__actions">
          <button class="btn btn--outline" @click="showStartNewProjectDialog = false">
            Cancel
          </button>
          <button class="btn btn--outline" @click="openHelpGuideStartingPoints">
            <span class="material-symbols-outlined btn__icon">menu_book</span>
            Open Help Guide
          </button>
          <button class="btn btn--primary" @click="goToProjectManagement">
            <span class="material-symbols-outlined btn__icon">settings</span>
            Go to Project Management
          </button>
        </div>
      </div>
    </div>
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

/* ── Editable project title ── */
/* Title row wraps the h1 + the inline edit block so they share width + stay
   aligned. The fallback ("Project Dashboard") is muted so a custom name pops. */
.dashboard__title-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
}

.dashboard__title {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  cursor: text;
  border-radius: var(--radius-sm);
  padding: 2px 4px;
  margin: -2px -4px; /* keep visual position identical to the old bare h1 */
  transition: background-color 0.15s;
}

.dashboard__title:hover {
  background-color: rgba(79, 70, 229, 0.06);
}

/* Custom name is rendered in the primary on-surface color so it reads as a
   real title; the fallback stays muted (via .page-title color) so the user
   notices the placeholder and is nudged to set a name. */
.dashboard__title--custom .dashboard__title-text {
  color: var(--color-on-surface);
}

/* Pencil affordance. Ghosted at rest (opacity 0.4) for a quiet title; full
   opacity on h1 hover so the affordance surfaces. */
.dashboard__title-edit-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  padding: 2px;
  cursor: pointer;
  color: var(--color-outline);
  border-radius: var(--radius-sm);
  opacity: 0.4;
  transition:
    opacity 0.15s,
    color 0.15s,
    background-color 0.15s;
  font-family: inherit;
}

.dashboard__title-edit-btn .material-symbols-outlined {
  font-size: 18px;
}

.dashboard__title:hover .dashboard__title-edit-btn {
  opacity: 1;
  color: #4f46e5;
}

.dashboard__title-edit-btn:hover {
  background-color: rgba(79, 70, 229, 0.1);
}

/* Edit-mode container sizing. Width in `ch` (the "0" glyph width at the
   title font size) so the field comfortably displays ~40 characters; capped
   at 100% so it never overflows narrow viewports. The inner input is `w-full`
   (from `clearable-input.vue`) and fills this width. */
.dashboard__title-edit {
  width: 42ch;
  max-width: 100%;
}

/* The ClearableInput's inner input: sized to the title font so the edit box
   reads as the same field the user just double-clicked. */
.dashboard__title-input {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
  padding: 6px 32px 6px 10px;
  background-color: #ffffff;
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

/* Header action group: "Start New Project" link + primary CTA side by side.
   On narrow viewports the header stacks vertically (see .dashboard__header
   responsive rule above), so this group wraps gracefully. */
.dashboard__header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
  justify-content: flex-end;
}

/* The "Start New Project" text-link: deliberately quiet (ghost/outline
   style) so the primary CTA (Connect LLM / Start Screening / Build Wiki)
   stays the visually dominant action. Turns indigo on hover. */
.dashboard__start-new-link {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  background: transparent;
  color: var(--color-on-surface-variant);
  border: 1px solid var(--color-border);
  padding: 8px 14px;
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition:
    color 0.15s,
    border-color 0.15s,
    background-color 0.15s;
  font-family: inherit;
  white-space: nowrap;
}

.dashboard__start-new-link:hover {
  color: #4f46e5;
  border-color: #c7d2fe;
  background-color: #eef2ff;
}

.dashboard__start-new-icon {
  font-size: 16px;
}

/* Numbered steps list inside the Start New Project dialog info-box. Tighter
   spacing than the default <ol> so it reads as a compact workflow. */
.dashboard__start-new-steps {
  margin: var(--space-2) 0 0 0;
  padding-left: 1.2rem;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.dashboard__start-new-steps li {
  font-size: var(--font-size-caption);
  line-height: var(--line-height-body);
  color: var(--color-on-surface-variant);
}

.dashboard__start-new-steps code {
  font-family: var(--font-family-mono);
  font-size: 12px;
  background-color: rgba(79, 70, 229, 0.08);
  padding: 1px 4px;
  border-radius: 3px;
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

/* ── Batch separator between "more" pages ── */
.activity-item__batch-separator {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.375rem 0;
}

.activity-item__batch-separator::before,
.activity-item__batch-separator::after {
  content: '';
  flex: 1;
  border-top: 1px solid #e2e8f0;
}

.activity-item__batch-separator-label {
  font-size: 0.6875rem;
  color: #94a3b8;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  white-space: nowrap;
}

/* ── TransitionGroup: staggered fade-in for newly loaded items ── */
.activity-item-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.activity-item-enter-active {
  transition:
    opacity 0.25s ease-out,
    transform 0.25s ease-out;
  transition-delay: var(--enter-delay, 0s);
}

.activity-item-move {
  transition: transform 0.3s ease;
}
</style>
