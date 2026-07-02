<script setup lang="ts">
defineProps<{
  completed: number;
  total: number;
  percentage: number;
  /** Tier 3 two-stage: optional stage sub-line (e.g. "Stage 2: 3/12 borderline"). */
  stage?: string | null;
}>();
</script>

<template>
  <div class="progress-bar">
    <div class="progress-bar__track">
      <div class="progress-bar__fill" :style="{ width: `${percentage}%` }" />
    </div>
    <div class="progress-bar__label">
      {{ completed }} / {{ total }} articles screened ({{ percentage }}%)
    </div>
    <div v-if="stage" class="progress-bar__stage">{{ stage }}</div>
  </div>
</template>

<style scoped>
.progress-bar {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.progress-bar__track {
  height: 16px;
  background-color: var(--color-surface-container-high);
  border-radius: var(--radius-pill);
  overflow: hidden;
  position: relative;
}

.progress-bar__fill {
  height: 100%;
  background-color: var(--color-primary);
  border-radius: var(--radius-pill);
  transition: width 0.5s ease;
  position: relative;
}

.progress-bar__label {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.progress-bar__stage {
  font-size: 11px;
  color: var(--color-primary);
  font-weight: 500;
  margin-top: -0.125rem;
}
</style>
