<script setup lang="ts">
import type { BatchRefScrapingProgress } from '../types';

defineProps<{
  progress: BatchRefScrapingProgress;
  percentage: number;
  done?: boolean;
}>();

defineEmits<{
  cancel: [];
  close: [];
}>();
</script>

<template>
  <div class="batch-ref-progress">
    <div class="batch-ref-progress__header">
      <div class="batch-ref-progress__stats">
        <span class="batch-ref-progress__label">
          {{ progress.completed }} / {{ progress.total }} articles processed ({{ percentage }}%)
        </span>
        <span
          v-if="progress.scraped"
          class="batch-ref-progress__stat batch-ref-progress__stat--scraped"
        >
          {{ progress.scraped }} scraped
        </span>
        <span
          v-if="progress.skipped"
          class="batch-ref-progress__stat batch-ref-progress__stat--skipped"
        >
          {{ progress.skipped }} skipped
        </span>
        <span
          v-if="progress.errors"
          class="batch-ref-progress__stat batch-ref-progress__stat--errors"
        >
          {{ progress.errors }} errors
        </span>
      </div>
      <!-- Cancel button (while running) - styled like AI Screening Stop -->
      <button
        v-if="!done"
        class="btn btn--danger"
        title="Cancel batch import"
        @click="$emit('cancel')"
      >
        Cancel
      </button>
      <!-- Close button (after completion) -->
      <button v-else class="batch-ref-progress__close" title="Close" @click="$emit('close')">
        <span class="material-symbols-outlined">close</span>
      </button>
    </div>

    <div class="batch-ref-progress__track">
      <div
        class="batch-ref-progress__fill"
        :class="{
          'batch-ref-progress__fill--error': progress.errors > 0,
          'batch-ref-progress__fill--done': done,
        }"
        :style="{ width: `${percentage}%` }"
      />
    </div>

    <div v-if="progress.currentArticleTitle && !done" class="batch-ref-progress__current">
      <span class="material-symbols-outlined batch-ref-progress__spinner">progress_activity</span>
      <span class="batch-ref-progress__title">{{ progress.currentArticleTitle }}</span>
    </div>
  </div>
</template>

<style scoped>
.batch-ref-progress {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-outline-variant);
}

.batch-ref-progress__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.batch-ref-progress__stats {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  flex-wrap: wrap;
}

.batch-ref-progress__label {
  font-size: var(--font-size-caption);
  font-weight: 600;
  color: var(--color-on-surface);
}

.batch-ref-progress__stat {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.batch-ref-progress__stat--scraped {
  color: var(--color-primary);
}

.batch-ref-progress__stat--skipped {
  color: var(--color-on-surface-variant);
}

.batch-ref-progress__stat--errors {
  color: var(--color-error);
}

/* Close button - icon-only, top-right */
.batch-ref-progress__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
  color: var(--color-on-surface-variant);
  background: none;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    color 0.15s ease;
}

.batch-ref-progress__close:hover {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}

.batch-ref-progress__close .material-symbols-outlined {
  font-size: 20px;
}

.batch-ref-progress__track {
  height: 12px;
  background-color: var(--color-surface-container-high);
  border-radius: var(--radius-pill);
  overflow: hidden;
}

.batch-ref-progress__fill {
  height: 100%;
  background-color: var(--color-primary);
  border-radius: var(--radius-pill);
  transition: width 0.4s ease;
}

.batch-ref-progress__fill--error {
  background-color: var(--color-error);
}

.batch-ref-progress__fill--done {
  background-color: var(--color-primary);
}

.batch-ref-progress__current {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.batch-ref-progress__spinner {
  font-size: 16px;
  animation: spin 1s linear infinite;
}

.batch-ref-progress__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 600px;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
