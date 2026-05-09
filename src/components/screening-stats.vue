<script setup lang="ts">
import { useRouter } from 'vue-router';

defineProps<{
  included: number;
  rejected: number;
  errors: number;
  estimatedTime: string;
}>();

const router = useRouter();

function goToArticles(status: string) {
  router.push({ path: '/articles', query: { status } });
}
</script>

<template>
  <div class="stats">
    <div class="stats__item stats__item--included" @click="goToArticles('included')">
      <span class="stats__value">{{ included }}</span>
      <span class="stats__label">Included</span>
      <span class="stats__link">View articles →</span>
    </div>
    <div class="stats__item stats__item--rejected" @click="goToArticles('rejected')">
      <span class="stats__value">{{ rejected }}</span>
      <span class="stats__label">Rejected</span>
      <span class="stats__link">View articles →</span>
    </div>
    <div class="stats__item stats__item--errors" @click="goToArticles('error')">
      <span class="stats__value">{{ errors }}</span>
      <span class="stats__label">Errors</span>
      <span class="stats__link">View articles →</span>
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

.stats__item--included,
.stats__item--rejected,
.stats__item--errors {
  cursor: pointer;
  transition:
    border-color 0.15s ease,
    box-shadow 0.15s ease;
}

.stats__item--included:hover,
.stats__item--rejected:hover,
.stats__item--errors:hover {
  border-color: var(--color-primary);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.08);
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

.stats__link {
  font-size: var(--font-size-caption);
  color: var(--color-primary);
  margin-top: var(--space-2);
  opacity: 0;
  transition: opacity 0.15s ease;
}

.stats__item--included:hover .stats__link,
.stats__item--rejected:hover .stats__link,
.stats__item--errors:hover .stats__link {
  opacity: 1;
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
