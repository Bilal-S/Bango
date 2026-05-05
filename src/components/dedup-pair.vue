<script setup lang="ts">
import type { DuplicatePair, DedupResolution } from '@/composables/use-dedup';

defineProps<{ pair: DuplicatePair }>();
const emit = defineEmits<{ resolve: [resolution: DedupResolution] }>();
</script>

<template>
  <div class="dedup-pair">
    <div class="dedup-pair__similarity">{{ (pair.similarity * 100).toFixed(1) }}% similar</div>
    <div class="dedup-pair__comparison">
      <div class="dedup-pair__record">
        <h3>Record A</h3>
        <p class="dedup-pair__title">{{ pair.articleATitle }}</p>
        <p class="dedup-pair__meta">{{ pair.articleAAuthors.join('; ') }}</p>
        <p class="dedup-pair__meta">{{ pair.articleAYear ?? 'No year' }}</p>
      </div>
      <div class="dedup-pair__vs">vs</div>
      <div class="dedup-pair__record">
        <h3>Record B</h3>
        <p class="dedup-pair__title">{{ pair.articleBTitle }}</p>
        <p class="dedup-pair__meta">{{ pair.articleBAuthors.join('; ') }}</p>
        <p class="dedup-pair__meta">{{ pair.articleBYear ?? 'No year' }}</p>
      </div>
    </div>
    <div class="dedup-pair__actions">
      <button class="btn btn--primary" @click="emit('resolve', 'keepA')">Keep A</button>
      <button class="btn btn--primary" @click="emit('resolve', 'keepB')">Keep B</button>
      <button class="btn btn--secondary" @click="emit('resolve', 'keepBoth')">Keep Both</button>
    </div>
  </div>
</template>

<style scoped>
.dedup-pair {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-default);
  padding: var(--space-4);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.dedup-pair__similarity {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
}

.dedup-pair__comparison {
  display: flex;
  gap: var(--space-3);
  align-items: stretch;
}

.dedup-pair__record {
  flex: 1;
  padding: var(--space-3);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-sm);
}

.dedup-pair__record h3 {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
  margin-bottom: var(--space-2);
}

.dedup-pair__title {
  font-weight: var(--font-weight-semibold);
  margin-bottom: var(--space-1);
}

.dedup-pair__meta {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.dedup-pair__vs {
  display: flex;
  align-items: center;
  font-size: var(--font-size-label);
  color: var(--color-outline);
  font-weight: var(--font-weight-semibold);
}

.dedup-pair__actions {
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
}

.btn {
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
}

.btn--primary {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.btn--secondary {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}
</style>
