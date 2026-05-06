<script setup lang="ts">
import { useDedup } from '@/composables/use-dedup';
import DedupPair from '@/components/dedup-pair.vue';
import type { DuplicatePair, DedupResolution } from '@/composables/use-dedup';

const { result, loading, error, resolvedCount, runDeduplication, resolveFuzzy } = useDedup();

function onResolve(pair: DuplicatePair, resolution: DedupResolution): void {
  resolveFuzzy(pair, resolution);
}
</script>

<template>
  <div class="dedup-view">
    <div class="dedup-view__header">
      <h1>Deduplication</h1>
      <button class="btn btn--primary" :disabled="loading" @click="runDeduplication">
        {{ loading ? 'Running...' : 'Run Deduplication' }}
      </button>
    </div>

    <div v-if="error" class="dedup-view__error">{{ error }}</div>

    <div v-if="result" class="dedup-view__content">
      <div class="dedup-view__summary">
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ result.autoMergedCount }}</span>
          <span class="dedup-view__stat-label">Auto-Merged</span>
        </div>
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ result.needsReviewCount }}</span>
          <span class="dedup-view__stat-label">Needs Review</span>
        </div>
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ resolvedCount }}</span>
          <span class="dedup-view__stat-label">Resolved</span>
        </div>
      </div>

      <section v-if="result.fuzzyMatches.length > 0">
        <h2>Potential Duplicates ({{ result.fuzzyMatches.length }} remaining)</h2>
        <div class="dedup-view__pairs">
          <DedupPair
            v-for="pair in result.fuzzyMatches"
            :key="`${pair.articleAId}-${pair.articleBId}`"
            :pair="pair"
            @resolve="(r: DedupResolution) => onResolve(pair, r)"
          />
        </div>
      </section>

      <div v-else-if="result.autoMergedCount > 0" class="dedup-view__done">
        <h2>Deduplication Complete</h2>
        <p>{{ result.autoMergedCount }} exact duplicates merged. No fuzzy matches found.</p>
      </div>

      <div v-else class="dedup-view__done">
        <h2>No Duplicates Found</h2>
        <p>All articles are unique. Articles have been advanced to Working status.</p>
      </div>
    </div>

    <div v-if="!result && !loading" class="dedup-view__empty">
      <p>Import articles first, then run deduplication to find and resolve duplicates.</p>
    </div>
  </div>
</template>

<style scoped>
.dedup-view {
  padding: var(--container-padding);
  max-width: 1000px;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .dedup-view {
    padding: var(--container-padding-sm);
  }

  .dedup-view__header {
    flex-direction: column;
    gap: var(--space-3);
    align-items: flex-start;
  }

  .dedup-view__summary {
    flex-wrap: wrap;
  }
}

.dedup-view__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-6);
}

.dedup-view__error {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
}

.dedup-view__summary {
  display: flex;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.dedup-view__stat {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  min-width: 100px;
}

.dedup-view__stat-value {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
}

.dedup-view__stat-label {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
}

.dedup-view__pairs {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-top: var(--space-4);
}

.dedup-view__done,
.dedup-view__empty {
  padding: var(--space-6);
  text-align: center;
  color: var(--color-on-surface-variant);
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

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
