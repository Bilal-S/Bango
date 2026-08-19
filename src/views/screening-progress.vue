<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useScreening } from '@/composables/use-screening';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { formatLlmError } from '@/utils/llm-error';
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

const router = useRouter();
const resettingWorkingList = ref(false);

/** Reactive LLM-configured gate. Lays instant Settings-edit reactivity over
 *  backend `readiness.hasLlmConfig`: Start button + guardrails read this so
 *  clearing the API key disables screening immediately. */
const llmConfigured = useLlmConfigured();

const screeningErrorInfo = computed(() => {
  if (!error.value) {
    return { prefix: '', details: '', helpLink: '', matched: false, anchorId: null };
  }
  return formatLlmError(error.value);
});

const isPaused = ref(false);
const BATCH_MIN = 1;
const BATCH_MAX = 15;
const NUM_TO_PROCESS_MIN = 1;
const batchSize = ref(1);
const numToProcess = ref<number | null>(null);

const totalUnscreened = computed(() => readiness.value?.totalUnscreened ?? 0);
const hasAvailableArticles = computed(() => totalUnscreened.value > 0);

const showBatchWarning = computed(() => batchSize.value > 1);
const canDecrement = computed(() => batchSize.value > BATCH_MIN);
const canIncrement = computed(() => batchSize.value < BATCH_MAX);
const canDecrementNumToProcess = computed(
  () => hasAvailableArticles.value && (numToProcess.value ?? 0) > NUM_TO_PROCESS_MIN
);
const canIncrementNumToProcess = computed(
  () => hasAvailableArticles.value && (numToProcess.value ?? 0) < totalUnscreened.value
);

watch(
  totalUnscreened,
  (total) => {
    if (total <= 0) {
      numToProcess.value = 0;
      return;
    }

    if (numToProcess.value === null || numToProcess.value <= 0) {
      numToProcess.value = total;
      return;
    }

    numToProcess.value = Math.min(total, Math.max(NUM_TO_PROCESS_MIN, numToProcess.value));
  },
  { immediate: true }
);

function decrementBatch(): void {
  if (canDecrement.value) {
    batchSize.value = Math.max(BATCH_MIN, batchSize.value - 1);
  }
}

function incrementBatch(): void {
  if (canIncrement.value) {
    batchSize.value = Math.min(BATCH_MAX, batchSize.value + 1);
  }
}

function decrementNumToProcess(): void {
  if (canDecrementNumToProcess.value) {
    numToProcess.value = Math.max(
      NUM_TO_PROCESS_MIN,
      (numToProcess.value ?? NUM_TO_PROCESS_MIN) - 1
    );
  }
}

function incrementNumToProcess(): void {
  if (canIncrementNumToProcess.value) {
    numToProcess.value = Math.min(
      totalUnscreened.value,
      (numToProcess.value ?? NUM_TO_PROCESS_MIN) + 1
    );
  }
}

/** Sanitize manual text-box entry to an integer in [BATCH_MIN, BATCH_MAX]. */
function onBatchInput(event: Event): void {
  const raw = (event.target as HTMLInputElement).value.replace(/[^0-9]/g, '');
  const parsed = parseInt(raw, 10);
  if (isNaN(parsed)) {
    batchSize.value = BATCH_MIN;
    return;
  }
  batchSize.value = Math.min(BATCH_MAX, Math.max(BATCH_MIN, parsed));
}

/** Sanitize manual text-box entry to an integer in [NUM_TO_PROCESS_MIN, totalUnscreened]. */
function onNumToProcessInput(event: Event): void {
  if (!hasAvailableArticles.value) {
    numToProcess.value = 0;
    return;
  }

  const raw = (event.target as HTMLInputElement).value.replace(/[^0-9]/g, '');
  const parsed = parseInt(raw, 10);
  if (isNaN(parsed)) {
    numToProcess.value = NUM_TO_PROCESS_MIN;
    return;
  }

  numToProcess.value = Math.min(totalUnscreened.value, Math.max(NUM_TO_PROCESS_MIN, parsed));
}

const displayNumToProcess = computed(
  () => numToProcess.value ?? (hasAvailableArticles.value ? totalUnscreened.value : 0)
);
const isRunning = computed(() => progress.value?.isRunning ?? false);
const displayAvailable = computed(() => readiness.value?.totalUnscreened ?? 0);
const displayRunningTotal = computed(() => progress.value?.total ?? 0);
const displayRunningCompleted = computed(() => progress.value?.completed ?? 0);
const controlsDisabled = computed(() => !hasAvailableArticles.value);
const effectiveNumToProcess = computed(() => {
  if (!hasAvailableArticles.value) return undefined;
  const requested = numToProcess.value ?? totalUnscreened.value;
  return Math.min(totalUnscreened.value, Math.max(NUM_TO_PROCESS_MIN, requested));
});

function onNumToProcessFocus(event: FocusEvent): void {
  const input = event.target as HTMLInputElement;
  input.select();
}

function onBatchInputFocus(event: FocusEvent): void {
  const input = event.target as HTMLInputElement;
  input.select();
}

function handleStart(): void {
  startScreening(batchSize.value, effectiveNumToProcess.value);
}

function navigateTo(route: string): void {
  router.push(route);
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

/** Can start screening: ANDs backend composite readiness with live
 *  `llmConfigured` gate so Start reacts instantly to Settings edits. */
const canStart = computed(() => {
  const r = readiness.value;
  if (!r) return false;
  return (
    r.totalUnscreened > 0 &&
    r.hasAims &&
    r.hasInclusion &&
    r.hasExclusion &&
    r.hasLlmConfig &&
    llmConfigured.value
  );
});

/** Blocking reasons (cascading: prerequisites first). */
const blockingReasons = computed((): string[] => {
  const r = readiness.value;
  if (!r) return []; // still loading - show spinner, not warnings

  const prereqReasons: string[] = [];
  if (!r.hasAims) prereqReasons.push('No research aims defined. Add aims in the Criteria Editor.');
  if (!r.hasInclusion)
    prereqReasons.push('No inclusion criteria defined. Add criteria in the Criteria Editor.');
  if (!r.hasExclusion)
    prereqReasons.push('No exclusion criteria defined. Add criteria in the Criteria Editor.');
  /* Surface LLM message when live gate is off even if stale readiness
   * snapshot still has `hasLlmConfig = true` (race: Settings edit after
   * last readiness fetch). */
  if (!r.hasLlmConfig || !llmConfigured.value)
    prereqReasons.push('LLM is not configured. Set up your LLM in Settings.');

  // Only surface the "no articles" warning once prerequisites are satisfied
  if (prereqReasons.length === 0 && r.totalUnscreened === 0) {
    return ['No unscreened articles in the working list. Import and deduplicate articles first.'];
  }
  return prereqReasons;
});

/** True when all prerequisites are met but no unscreened articles exist.
 *  Includes live `llmConfigured` gate. */
const isWorkingListScreened = computed((): boolean => {
  const r = readiness.value;
  if (!r) return false;
  return (
    r.hasAims &&
    r.hasInclusion &&
    r.hasExclusion &&
    r.hasLlmConfig &&
    llmConfigured.value &&
    r.totalUnscreened === 0
  );
});
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
            <p class="screening-view__subtitle">
              <template
                v-if="isRunning && progress?.phase && progress.phase.startsWith('preparing:')"
              >
                Preparing: <strong>{{ progress.stage ?? 'working…' }}</strong>
              </template>
              <template v-else-if="isRunning">
                Processing: <strong>{{ displayRunningCompleted }}</strong> of
                <strong>{{ displayRunningTotal }}</strong> article(s)
              </template>
              <template v-else>
                Available: <strong>{{ displayAvailable }}</strong> article(s)
              </template>
            </p>
          </div>
          <div v-if="isRunning && progress && progress.total > 0" class="screening-view__percent">
            <span class="screening-view__percent-value">{{ percentage }}%</span>
            <span class="screening-view__percent-label">Completion</span>
          </div>
        </div>

        <!-- Non-fatal warning banner (e.g. slow LLM, first timeout) -->
        <div
          v-if="progress?.warning && !progress?.fatalError"
          class="screening-view__warning-banner"
        >
          <span class="material-symbols-outlined">warning</span>
          <span>{{ progress.warning }}</span>
        </div>

        <!-- Fatal error banner (auth failure, consecutive transient failures) -->
        <div v-if="progress?.fatalError" class="screening-view__fatal-error">
          <span class="material-symbols-outlined">error</span>
          <span>{{ progress.fatalError }}</span>
        </div>

        <!-- Deferred articles notice (transient LLM errors left them unscreened) -->
        <div
          v-if="progress?.deferred && progress.deferred > 0"
          class="screening-view__deferred-notice"
        >
          <span class="material-symbols-outlined">schedule</span>
          <span
            >{{ progress.deferred }} article(s) deferred (LLM was unavailable) - they'll be screened
            on the next run.</span
          >
        </div>

        <ScreeningProgressBar
          v-if="isRunning && progress && progress.total > 0"
          :completed="progress.completed"
          :total="progress.total"
          :percentage="percentage"
          :stage="progress.stage"
          :phase="progress.phase"
        />
      </section>

      <!-- Error Banner -->
      <div v-if="error" class="screening-view__error">
        <div class="screening-view__error-block">
          <p class="screening-view__error-prefix">{{ screeningErrorInfo.prefix }}</p>
          <p class="screening-view__error-details">{{ screeningErrorInfo.details }}</p>
          <a class="screening-view__error-link" :href="screeningErrorInfo.helpLink">
            <span class="material-symbols-outlined" style="font-size: 14px; margin-right: 4px"
              >open_in_new</span
            >
            View Troubleshooting Guide
          </a>
        </div>
      </div>

      <!-- Token Warning -->
      <div v-if="tokenWarning" class="screening-view__warning">
        {{ tokenWarning }}
      </div>

      <!-- Blocking Reasons (guardrails) -->
      <div
        v-if="blockingReasons.length > 0 && !progress?.isRunning && !isWorkingListScreened"
        class="screening-view__guardrails"
      >
        <p class="screening-view__guardrails-title">Before screening, address the following:</p>
        <ul>
          <li v-for="(reason, idx) in blockingReasons" :key="idx">{{ reason }}</li>
        </ul>
        <!-- Actionable LLM config guidance. Also shows when the live
             `llmConfigured` gate is off even if the stale readiness snapshot
             still reports `hasLlmConfig = true` (race: Settings edit happened
             after the last readiness fetch). -->
        <div
          v-if="readiness && (!readiness.hasLlmConfig || !llmConfigured)"
          class="screening-view__llm-setup-card"
        >
          <span class="material-symbols-outlined screening-view__llm-setup-icon">smart_toy</span>
          <div class="screening-view__llm-setup-body">
            <p class="screening-view__llm-setup-text">
              Set up an AI provider and API key in <strong>Settings</strong> to enable screening.
              Both cloud providers (OpenAI, Anthropic, Google) and local models (Ollama, LM Studio)
              are supported.
            </p>
            <div class="screening-view__llm-setup-actions">
              <button class="btn btn--primary" @click="navigateTo('/settings')">
                <span class="material-symbols-outlined" style="font-size: 16px">settings</span>
                Open Settings
              </button>
              <button class="btn btn--secondary" @click="navigateTo('/help?tab=local-ai')">
                <span class="material-symbols-outlined" style="font-size: 16px">help</span>
                Setup Guide
              </button>
            </div>
          </div>
        </div>
        <!-- Actionable criteria guidance -->
        <div
          v-if="
            readiness && (!readiness.hasAims || !readiness.hasInclusion || !readiness.hasExclusion)
          "
          class="screening-view__llm-setup-card"
        >
          <span class="material-symbols-outlined screening-view__llm-setup-icon">rule</span>
          <div class="screening-view__llm-setup-body">
            <p class="screening-view__llm-setup-text">
              Define your research aims and inclusion/exclusion criteria in the
              <strong>Criteria Editor</strong>.
            </p>
            <div class="screening-view__llm-setup-actions">
              <button class="btn btn--primary" @click="navigateTo('/criteria')">
                <span class="material-symbols-outlined" style="font-size: 16px">edit</span>
                Open Criteria Editor
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Stats Grid -->
      <ScreeningStats
        v-if="progress && progress.total > 0"
        :included="progress.included"
        :rejected="progress.rejected"
        :errors="progress.errors"
        :estimated-time="estimatedTimeRemaining"
      />

      <!-- Currently Screening Indicator -->
      <div v-if="progress?.currentArticleTitles?.length" class="screening-view__current">
        <span class="screening-view__current-dot" />
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[11px] text-slate-500 font-semibold uppercase tracking-wider">
            {{
              progress.currentArticleTitles.length === 1
                ? 'Screening'
                : `Screening (${progress.currentArticleTitles.length} articles)`
            }}
          </span>
          <span
            v-for="(title, idx) in progress.currentArticleTitles"
            :key="idx"
            class="text-sm text-slate-700 truncate"
          >
            {{ title }}
          </span>
        </div>
      </div>

      <!-- Start Configuration Controls (only when not running) -->
      <div v-if="!isRunning && !loading" class="screening-view__start-config">
        <div class="screening-view__start-config-row">
          <div
            class="screening-view__batch screening-view__batch--half"
            data-testid="num-to-process-tile"
          >
            <div class="screening-view__batch-header">
              <label class="screening-view__batch-label" for="num-to-process-input">
                To Process
              </label>
              <span class="screening-view__batch-value">{{ displayNumToProcess }}</span>
            </div>
            <div class="screening-view__stepper">
              <button
                type="button"
                class="screening-view__stepper-btn"
                :disabled="controlsDisabled || !canDecrementNumToProcess"
                aria-label="Decrease number to process"
                @click="decrementNumToProcess"
              >
                <span class="material-symbols-outlined">remove</span>
              </button>
              <input
                id="num-to-process-input"
                maxlength="4"
                class="screening-view__stepper-input"
                type="text"
                inputmode="numeric"
                pattern="[0-9]*"
                :value="displayNumToProcess"
                :disabled="controlsDisabled"
                aria-label="Number to process"
                @focus="onNumToProcessFocus"
                @input="onNumToProcessInput"
              />
              <button
                type="button"
                class="screening-view__stepper-btn"
                :disabled="controlsDisabled || !canIncrementNumToProcess"
                aria-label="Increase number to process"
                @click="incrementNumToProcess"
              >
                <span class="material-symbols-outlined">add</span>
              </button>
            </div>
          </div>

          <div
            class="screening-view__batch screening-view__batch--half"
            data-testid="batch-size-tile"
          >
            <div class="screening-view__batch-header">
              <label class="screening-view__batch-label" for="batch-input"> Batch Size </label>
              <span class="screening-view__batch-value">{{ batchSize }}</span>
            </div>
            <div class="screening-view__stepper">
              <button
                type="button"
                class="screening-view__stepper-btn"
                :disabled="controlsDisabled || !canDecrement"
                aria-label="Decrease batch size"
                @click="decrementBatch"
              >
                <span class="material-symbols-outlined">remove</span>
              </button>
              <input
                id="batch-input"
                class="screening-view__stepper-input"
                type="text"
                inputmode="numeric"
                pattern="[0-9]*"
                :value="batchSize"
                :disabled="controlsDisabled"
                aria-label="Batch size"
                @focus="onBatchInputFocus"
                @input="onBatchInput"
              />
              <button
                type="button"
                class="screening-view__stepper-btn"
                :disabled="controlsDisabled || !canIncrement"
                aria-label="Increase batch size"
                @click="incrementBatch"
              >
                <span class="material-symbols-outlined">add</span>
              </button>
            </div>
            <div v-if="showBatchWarning && !controlsDisabled" class="screening-view__batch-warning">
              ⚠ Batching may not be supported by your LLM and may lead to failures.
            </div>
          </div>
        </div>
      </div>

      <!-- Controls -->
      <div class="screening-view__controls">
        <div class="screening-view__actions">
          <button
            v-if="!progress?.isRunning && !loading"
            class="btn btn--primary"
            :disabled="!canStart || controlsDisabled"
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
        <button
          v-if="!progress?.isRunning && !loading && isWorkingListScreened"
          class="btn btn--primary"
          @click="navigateTo('/articles')"
        >
          Goto articles
          <span class="material-symbols-outlined" style="font-size: 16px">arrow_forward</span>
        </button>
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

  .screening-view__start-config-row {
    grid-template-columns: 1fr;
  }
}

.screening-view__start-config {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.screening-view__start-config-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-3);
}

.screening-view__batch--half {
  min-width: 0;
}

.screening-view__stepper-input:disabled,
.screening-view__stepper-btn:disabled {
  cursor: not-allowed;
}

.screening-view__stepper-input:disabled {
  opacity: 0.6;
}

.screening-view__subtitle strong {
  font-weight: var(--font-weight-semibold);
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

.screening-view__error-block {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.screening-view__error-prefix {
  font-weight: 500;
  margin: 0;
}

.screening-view__error-details {
  font-family: ui-monospace, SFMono-Regular, monospace;
  font-size: 12px;
  background-color: rgba(153, 27, 27, 0.08);
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  margin: 0;
  word-break: break-word;
}

.screening-view__error-link {
  display: inline-flex;
  align-items: center;
  color: #4f46e5;
  font-weight: 500;
  text-decoration: none;
  font-size: 13px;
}

.screening-view__error-link:hover {
  text-decoration: underline;
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

/* ── Actionable Setup Cards (inside guardrails) ── */
.screening-view__llm-setup-card {
  display: flex;
  gap: var(--space-3);
  margin-top: var(--space-3);
  padding: var(--space-3) var(--space-4);
  background-color: #fffbeb;
  border: 1px solid #fde68a;
  border-radius: var(--radius-default);
}

.screening-view__llm-setup-icon {
  font-size: 20px;
  color: #d97706;
  flex-shrink: 0;
  margin-top: 2px;
}

.screening-view__llm-setup-body {
  flex: 1;
  min-width: 0;
}

.screening-view__llm-setup-text {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: var(--line-height-body);
  margin: 0 0 var(--space-3) 0;
}

.screening-view__llm-setup-actions {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
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
  align-self: flex-start;
  margin-top: 4px;
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

/* ── Batch Size ── */
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

/* ── Batch Size Stepper ── */
.screening-view__stepper {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
}

.screening-view__stepper-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: var(--radius-default);
  border: 1px solid var(--color-border);
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    opacity 0.15s ease;
  font-family: var(--font-family);
}

.screening-view__stepper-btn .material-symbols-outlined {
  font-size: 20px;
  line-height: 1;
  user-select: none;
}

.screening-view__stepper-btn:hover:not(:disabled) {
  background-color: var(--color-surface-container-highest);
}

.screening-view__stepper-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.screening-view__stepper-input {
  width: 7.5ch;
  text-align: center;
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  color: var(--color-on-surface);
  background-color: var(--color-surface-container-lowest, #ffffff);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-1) var(--space-2);
  outline: none;
  transition: border-color 0.15s ease;
  appearance: textfield;
}

.screening-view__stepper-input::-webkit-outer-spin-button,
.screening-view__stepper-input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

.screening-view__stepper-input:focus {
  border-color: var(--color-primary);
}

.screening-view__batch-warning {
  font-size: var(--font-size-caption);
  color: var(--color-priority-high);
  margin-top: var(--space-1);
}

/* ── Fatal Error Banner ── */
.screening-view__fatal-error {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-medium);
  line-height: var(--line-height-body);
}

.screening-view__fatal-error .material-symbols-outlined {
  font-size: 20px;
  flex-shrink: 0;
  margin-top: 2px;
}

/* ── Warning Banner (non-fatal: slow LLM, first timeout) ── */
.screening-view__warning-banner {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-warning-container, #fef3cd);
  color: var(--color-on-warning-container, #664d03);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-medium);
  line-height: var(--line-height-body);
}

.screening-view__warning-banner .material-symbols-outlined {
  font-size: 20px;
  flex-shrink: 0;
  margin-top: 2px;
}

/* ── Deferred Articles Notice ── */
.screening-view__deferred-notice {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  color: var(--color-on-surface-variant);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  line-height: var(--line-height-body);
}

.screening-view__deferred-notice .material-symbols-outlined {
  font-size: 20px;
  flex-shrink: 0;
  margin-top: 2px;
}
</style>
