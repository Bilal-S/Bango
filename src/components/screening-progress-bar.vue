<script setup lang="ts">
defineProps<{
  completed: number;
  total: number;
  percentage: number;
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
</style>
