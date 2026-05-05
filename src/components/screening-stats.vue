<script setup lang="ts">
defineProps<{
  included: number;
  rejected: number;
  errors: number;
  estimatedTime: string;
}>();
</script>

<template>
  <div class="stats">
    <div class="stats__item stats__item--included">
      <span class="stats__value">{{ included }}</span>
      <span class="stats__label">Included</span>
    </div>
    <div class="stats__item stats__item--rejected">
      <span class="stats__value">{{ rejected }}</span>
      <span class="stats__label">Rejected</span>
    </div>
    <div class="stats__item stats__item--errors">
      <span class="stats__value">{{ errors }}</span>
      <span class="stats__label">Errors</span>
    </div>
    <div class="stats__item">
      <span class="stats__value">{{ estimatedTime }}</span>
      <span class="stats__label">Est. Remaining</span>
    </div>
  </div>
</template>

<style scoped>
.stats {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--space-4);
}

.stats__item {
  display: flex;
  flex-direction: column;
  padding: var(--space-4) var(--space-5);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
}

.stats__value {
  font-size: var(--font-size-display);
  font-weight: var(--font-weight-semibold);
  line-height: var(--line-height-display);
  color: var(--color-on-surface);
}

.stats__label {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  margin-top: var(--space-1);
}

.stats__item--included .stats__value {
  color: #16a34a;
}

.stats__item--rejected .stats__value {
  color: var(--color-on-surface-variant);
}

.stats__item--errors .stats__value {
  color: var(--color-error);
}
</style>
