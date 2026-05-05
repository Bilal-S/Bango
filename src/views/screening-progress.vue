<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { useScreening } from '@/composables/use-screening';
import ScreeningProgressBar from '@/components/screening-progress-bar.vue';
import ScreeningStats from '@/components/screening-stats.vue';

const {
  progress,
  loading,
  error,
  tokenWarning,
  percentage,
  estimatedTimeRemaining,
  startScreening,
  pauseScreening,
  resumeScreening,
  stopScreening,
  checkTokenEstimate,
  refreshProgress,
} = useScreening();

const isPaused = ref(false);

let pollInterval: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  checkTokenEstimate();
  refreshProgress();
});

onUnmounted(() => {
  if (pollInterval) {
    clearInterval(pollInterval);
  }
});

function handleStart(): void {
  startScreening();
  // Poll progress while running
  pollInterval = setInterval(() => {
    if (progress.value?.isRunning) {
      refreshProgress();
    } else if (pollInterval) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  }, 2000);
}
</script>

<template>
  <div class="screening-view">
    <!-- Hero Progress Section -->
    <section class="screening-view__hero">
      <div class="screening-view__hero-header">
        <div>
          <h1 class="screening-view__title">AI Screening</h1>
          <p v-if="progress" class="screening-view__subtitle">
            Processing: <strong>{{ progress.completed }}</strong> / {{ progress.total }} articles
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

    <!-- Controls -->
    <div class="screening-view__controls">
      <div class="screening-view__actions">
        <button
          v-if="!progress?.isRunning && !loading"
          class="btn btn--primary"
          @click="handleStart"
        >
          Start Screening
        </button>
        <button v-if="loading" class="btn btn--primary" disabled>Starting...</button>
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
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="!progress && !loading" class="screening-view__empty">
      <p>Configure your criteria and LLM settings, then start screening.</p>
      <button class="btn btn--secondary" @click="checkTokenEstimate">Estimate Token Usage</button>
    </div>
  </div>
</template>

<style scoped>
.screening-view {
  padding: var(--space-8);
  max-width: 960px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
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

.screening-view__title {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  line-height: var(--line-height-display);
  letter-spacing: var(--letter-spacing-display);
  color: var(--color-on-surface);
  margin: 0;
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
</style>
