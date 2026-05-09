<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useScreening } from '@/composables/use-screening';
import ScreeningProgressBar from '@/components/screening-progress-bar.vue';
import ScreeningStats from '@/components/screening-stats.vue';

const {
  progress,
  loading,
  readinessLoading,
  error,
  tokenWarning,
  readiness,
  percentage,
  estimatedTimeRemaining,
  fetchReadiness,
  startScreening,
  pauseScreening,
  resumeScreening,
  stopScreening,
  stopListening,
  startListening,
  resetScreeningErrors,
  resetWorkingList,
} = useScreening();

const resettingWorkingList = ref(false);

const isPaused = ref(false);
const batchSize = ref(1);
const showBatchWarning = computed(() => batchSize.value > 4);

onMounted(async () => {
  // Silent background refresh
  await fetchReadiness();
  // If a screening run is already in progress, start listening for events
  if (progress.value?.isRunning) {
    await startListening();
  }
});

onUnmounted(() => {
  stopListening();
});

/** Computed: can the user start screening? */
const canStart = computed(() => {
  const r = readiness.value;
  if (!r) return false;
  return r.totalUnscreened > 0 && r.hasAims && r.hasInclusion && r.hasExclusion && r.hasLlmConfig;
});

/** Computed: list of blocking reasons to show the user (cascading: prerequisites first). */
const blockingReasons = computed((): string[] => {
  const r = readiness.value;
  if (!r) return []; // still loading — show spinner, not warnings

  const prereqReasons: string[] = [];
  if (!r.hasAims) prereqReasons.push('No research aims defined. Add aims in the Criteria Editor.');
  if (!r.hasInclusion)
    prereqReasons.push('No inclusion criteria defined. Add criteria in the Criteria Editor.');
  if (!r.hasExclusion)
    prereqReasons.push('No exclusion criteria defined. Add criteria in the Criteria Editor.');
  if (!r.hasLlmConfig) prereqReasons.push('LLM is not configured. Set up your LLM in Settings.');

  // Only surface the "no articles" warning once prerequisites are satisfied
  if (prereqReasons.length === 0 && r.totalUnscreened === 0) {
    return ['No unscreened articles in the working list. Import and deduplicate articles first.'];
  }
  return prereqReasons;
});

/** Display count for the hero subtitle. */
const displayTotal = computed((): number => {
  if (progress.value && progress.value.total > 0) return progress.value.total;
  return readiness.value?.totalUnscreened ?? 0;
});

const displayCompleted = computed((): number => {
  return progress.value?.completed ?? 0;
});

/** Computed: true when all prerequisites are met but no unscreened articles exist. */
const isWorkingListScreened = computed((): boolean => {
  const r = readiness.value;
  if (!r) return false;
  return r.hasAims && r.hasInclusion && r.hasExclusion && r.hasLlmConfig && r.totalUnscreened === 0;
});

function handleStart(): void {
  startScreening(batchSize.value);
}

async function handleResetWorkingList(): Promise<void> {
  resettingWorkingList.value = true;
  try {
    await resetWorkingList();
  } catch {
    // Error is handled by the composable
  } finally {
    resettingWorkingList.value = false;
  }
}
</script>

<template>
  <div class="screening-view">
    <!-- Non-blocking Loading Indicator (shown if refreshing in background) -->
    <div
      v-if="readinessLoading && readiness"
      class="screening-view__refreshing-hint"
      title="Refreshing screening readiness data..."
    >
      <div class="screening-view__spinner-sm" />
    </div>

    <!-- Initial Loading State (only if no data at all) -->
    <div v-if="readinessLoading && !readiness" class="screening-view__loading">
      <div class="screening-view__spinner" />
      <p>Loading screening data&hellip;</p>
    </div>

    <template v-else-if="readiness">
      <!-- Hero Progress Section -->
      <section class="screening-view__hero">
        <div class="screening-view__hero-header">
          <div>
            <h1 class="page-title">AI Screening</h1>
            <p v-if="displayTotal > 0" class="screening-view__subtitle">
              Processing: <strong>{{ displayCompleted }}</strong> / {{ displayTotal }} articles
            </p>
            <p v-else class="screening-view__subtitle">
              Screen articles against your inclusion/exclusion criteria
            </p>
          </div>
          <div v-if="progress && progress.total > 0" class="screening-view__percent">
            <span class="screening-view__percent-value">{{ percentage }}%</span>
            <span class="screening-view__percent-label">Completion</span>
          </div>
        </div>

        <ScreeningProgressBar
          v-if="progress && progress.total > 0"
          :completed="progress.completed"
          :total="progress.total"
          :percentage="percentage"
        />
      </section>

      <!-- Error Banner -->
      <div v-if="error" class="screening-view__error">
        {{ error }}
      </div>

      <!-- Token Warning -->
      <div v-if="tokenWarning" class="screening-view__warning">
        {{ tokenWarning }}
      </div>

      <!-- Blocking Reasons (guardrails) -->
      <div
        v-if="blockingReasons.length > 0 && !progress?.isRunning"
        class="screening-view__guardrails"
      >
        <p class="screening-view__guardrails-title">Before screening, address the following:</p>
        <ul>
          <li v-for="(reason, idx) in blockingReasons" :key="idx">{{ reason }}</li>
        </ul>
      </div>

      <!-- Stats Grid -->
      <ScreeningStats
        v-if="progress && progress.total > 0"
        :included="progress.included"
        :rejected="progress.rejected"
        :errors="progress.errors"
        :estimated-time="estimatedTimeRemaining"
      />

      <!-- Current Article Indicator -->
      <div v-if="progress?.currentArticleTitle" class="screening-view__current">
        <span class="screening-view__current-dot" />
        Screening: {{ progress.currentArticleTitle }}
      </div>

      <!-- Batch Size Control (only when not running) -->
      <div v-if="!progress?.isRunning && !loading" class="screening-view__batch">
        <div class="screening-view__batch-header">
          <label class="screening-view__batch-label" for="batch-slider"> Batch Size </label>
          <span class="screening-view__batch-value">{{ batchSize }}</span>
        </div>
        <input
          id="batch-slider"
          v-model.number="batchSize"
          type="range"
          min="1"
          max="15"
          step="1"
          class="screening-view__batch-slider"
        />
        <div class="screening-view__batch-range">
          <span>1</span>
          <span>15</span>
        </div>
        <div v-if="showBatchWarning" class="screening-view__batch-warning">
          ⚠ High batch sizes might not be supported by your LLM and may lead to failures.
        </div>
      </div>

      <!-- Controls -->
      <div class="screening-view__controls">
        <div class="screening-view__actions">
          <button
            v-if="!progress?.isRunning && !loading"
            class="btn btn--primary"
            :disabled="!canStart"
            @click="handleStart"
          >
            Start Screening
          </button>
          <button
            v-if="
              !progress?.isRunning && !loading && isWorkingListScreened && !resettingWorkingList
            "
            class="btn btn--secondary"
            @click="handleResetWorkingList"
          >
            Refresh from Working List
          </button>
          <button v-if="resettingWorkingList" class="btn btn--secondary" disabled>
            <span class="screening-view__btn-spinner" />
            Refreshing&hellip;
          </button>
          <button v-if="loading" class="btn btn--primary" disabled>
            <span class="screening-view__btn-spinner" />
            Starting&hellip;
          </button>
          <span v-if="progress?.isRunning" class="screening-view__activity-spinner">
            <span class="screening-view__btn-spinner screening-view__btn-spinner--inline" />
            <span class="screening-view__activity-label">Screening&hellip;</span>
          </span>
          <button
            v-if="progress?.isRunning && !isPaused"
            class="btn btn--primary"
            @click="
              pauseScreening();
              isPaused = true;
            "
          >
            Pause
          </button>
          <button
            v-if="progress?.isRunning && isPaused"
            class="btn btn--primary"
            @click="
              resumeScreening();
              isPaused = false;
            "
          >
            Resume
          </button>
          <button v-if="progress?.isRunning" class="btn btn--danger" @click="stopScreening">
            Stop
          </button>
          <button
            v-if="!progress?.isRunning && !loading && progress?.errors && progress.errors > 0"
            class="btn btn--secondary"
            @click="resetScreeningErrors"
          >
            Clear Errors ({{ progress.errors }}) & Re-screen
          </button>
        </div>
      </div>

      <!-- Empty State (when no progress and no guardrails — shouldn't normally show but fallback) -->
      <div
        v-if="!progress && !loading && readiness && blockingReasons.length === 0"
        class="screening-view__empty"
      >
        <p>Configure your criteria and LLM settings, then start screening.</p>
      </div>
    </template>
  </div>
</template>

<style scoped>
.screening-view {
  padding: var(--container-padding);
  max-width: 960px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
}

@media (max-width: 767px) {
  .screening-view {
    padding: var(--container-padding-sm);
    gap: var(--space-4);
  }

  .screening-view__hero-header {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .screening-view__controls {
    flex-direction: column;
    gap: var(--space-3);
  }

  .screening-view__actions {
    flex-wrap: wrap;
  }
}

.screening-view__loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-4);
  padding: var(--space-16) 0;
  color: var(--color-on-surface-variant);
}

.screening-view__spinner {
  width: 32px;
  height: 32px;
  border: 3px solid var(--color-surface-container-highest);
  border-top-color: var(--color-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.screening-view__refreshing-hint {
  position: absolute;
  top: var(--space-4);
  right: var(--space-4);
  z-index: 10;
}

.screening-view__spinner-sm {
  width: 16px;
  height: 16px;
  border: 2px solid var(--color-surface-container-highest);
  border-top-color: var(--color-primary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

.screening-view__hero {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.screening-view__hero-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
}

.screening-view__subtitle {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  margin: var(--space-1) 0 0;
}

.screening-view__percent {
  text-align: right;
}

.screening-view__percent-value {
  font-size: 36px;
  font-weight: var(--font-weight-semibold);
  color: var(--color-primary);
  line-height: 1;
}

.screening-view__percent-label {
  display: block;
  font-size: var(--font-size-label);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
  margin-top: var(--space-1);
}

.screening-view__error {
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
}

.screening-view__warning {
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  color: var(--color-priority-high);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  border: 1px solid var(--color-priority-high);
}

.screening-view__guardrails {
  padding: var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  border-left: 3px solid var(--color-priority-high);
}

.screening-view__guardrails-title {
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  margin: 0 0 var(--space-2);
}

.screening-view__guardrails ul {
  margin: 0;
  padding-left: var(--space-5);
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.screening-view__current {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  font-style: italic;
}

.screening-view__current-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--color-primary);
  animation: pulse 1.5s ease-in-out infinite;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}

.screening-view__controls {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding-top: var(--space-4);
  border-top: 1px solid var(--color-border);
}

.screening-view__actions {
  display: flex;
  gap: var(--space-3);
}

.screening-view__btn-spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--color-on-primary);
  border-top-color: transparent;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  display: inline-block;
}

.screening-view__btn-spinner--inline {
  border-color: var(--color-primary);
  border-top-color: transparent;
}

.screening-view__activity-spinner {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.screening-view__activity-label {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  font-weight: var(--font-weight-medium);
}

.screening-view__empty {
  text-align: center;
  color: var(--color-on-surface-variant);
  padding: var(--space-10) 0;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-5);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  border: 1px solid transparent;
  transition: all 0.15s ease;
  font-family: var(--font-family);
}

.btn--primary {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.btn--primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn--danger {
  background-color: var(--color-surface-container-low);
  color: var(--color-error);
  border-color: var(--color-border);
}

.btn--danger:hover:not(:disabled) {
  background-color: var(--color-error-container);
}

.btn--secondary {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}

.btn--secondary:hover:not(:disabled) {
  background-color: var(--color-surface-container-highest);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ── Batch Size Slider ── */
.screening-view__batch {
  padding: var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.screening-view__batch-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.screening-view__batch-label {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
}

.screening-view__batch-value {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  color: var(--color-primary);
  min-width: 2ch;
  text-align: right;
}

.screening-view__batch-slider {
  -webkit-appearance: none;
  appearance: none;
  width: 100%;
  height: 6px;
  border-radius: 3px;
  background: var(--color-surface-container-highest);
  outline: none;
  cursor: pointer;
}

.screening-view__batch-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--color-primary);
  cursor: pointer;
  border: 2px solid var(--color-on-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.screening-view__batch-slider::-moz-range-thumb {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--color-primary);
  cursor: pointer;
  border: 2px solid var(--color-on-primary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

.screening-view__batch-range {
  display: flex;
  justify-content: space-between;
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.screening-view__batch-warning {
  font-size: var(--font-size-caption);
  color: var(--color-priority-high);
  margin-top: var(--space-1);
}
</style>
