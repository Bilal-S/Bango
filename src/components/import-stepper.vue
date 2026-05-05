<script setup lang="ts">
import type { ImportStep } from '@/composables/use-import';

defineProps<{ currentStep: ImportStep }>();

const steps: { key: ImportStep; label: string }[] = [
  { key: 'upload', label: 'Upload' },
  { key: 'parse', label: 'Parse' },
  { key: 'import', label: 'Review' },
  { key: 'complete', label: 'Complete' },
];

function stepIndex(step: ImportStep): number {
  return steps.findIndex((s) => s.key === step);
}
</script>

<template>
  <div class="stepper">
    <div
      v-for="(step, i) in steps"
      :key="step.key"
      class="stepper__step"
      :class="{
        'stepper__step--active': stepIndex(currentStep) === i,
        'stepper__step--done': stepIndex(currentStep) > i,
      }"
    >
      <div class="stepper__dot">
        <span v-if="stepIndex(currentStep) > i" class="material-symbols-outlined">check</span>
        <template v-else>{{ i + 1 }}</template>
      </div>
      <span class="stepper__label">{{ step.label }}</span>
      <div v-if="i < steps.length - 1" class="stepper__line" />
    </div>
  </div>
</template>

<style scoped>
.stepper {
  display: flex;
  align-items: center;
  gap: 0;
  padding: var(--space-4) 0;
}

.stepper__step {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex: 1;
}

.stepper__dot {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface-variant);
  flex-shrink: 0;
}

.stepper__step--active .stepper__dot {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.stepper__step--done .stepper__dot {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.stepper__label {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  white-space: nowrap;
}

.stepper__step--active .stepper__label {
  color: var(--color-on-surface);
  font-weight: var(--font-weight-semibold);
}

.stepper__line {
  flex: 1;
  height: 1px;
  background-color: var(--color-outline-variant);
  margin: 0 var(--space-2);
}
</style>
