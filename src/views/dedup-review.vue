<script setup lang="ts">
import { onMounted } from 'vue';
import { useDedup } from '@/composables/use-dedup';
import DedupPair from '@/components/dedup-pair.vue';
import type { DuplicatePair, DedupResolution } from '@/composables/use-dedup';

const {
  result,
  loading,
  error,
  resolvedCount,
  mergedCount,
  checkDuplicates,
  mergeAllExact,
  resolveFuzzy,
} = useDedup();

function onResolve(pair: DuplicatePair, resolution: DedupResolution): void {
  resolveFuzzy(pair, resolution);
}

onMounted(() => {
  if (!result.value) {
    checkDuplicates();
  }
});
</script>

<template>
  <div class="dedup-view">
    <div class="dedup-view__header">
      <h1 class="page-title">Deduplication</h1>
      <button class="btn btn--secondary" :disabled="loading" @click="checkDuplicates">
        {{ loading ? 'Checking...' : 'Re-check Duplicates' }}
      </button>
    </div>

    <div v-if="error" class="dedup-view__error">{{ error }}</div>

    <div v-if="result" class="dedup-view__content">
      <!-- Summary stats -->
      <div class="dedup-view__summary">
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ result.autoMergedCount }}</span>
          <span class="dedup-view__stat-label">Exact Duplicates</span>
        </div>
        <div class="dedup-view__stat">
          <span class="dedup-view__stat-value">{{ result.needsReviewCount }}</span>
          <span class="dedup-view__stat-label">Needs Review</span>
        </div>
        <div v-if="mergedCount > 0" class="dedup-view__stat dedup-view__stat--done">
          <span class="dedup-view__stat-value">{{ mergedCount }}</span>
          <span class="dedup-view__stat-label">Merged</span>
        </div>
        <div v-if="resolvedCount > 0" class="dedup-view__stat dedup-view__stat--done">
          <span class="dedup-view__stat-value">{{ resolvedCount }}</span>
          <span class="dedup-view__stat-label">Resolved</span>
        </div>
      </div>

      <!-- Section 1: High-Confidence Exact Duplicates -->
      <section v-if="result.exactDuplicates.length > 0" class="dedup-view__section">
        <div class="dedup-view__section-header">
          <h2>High-Confidence Duplicates ({{ result.exactDuplicates.length }})</h2>
          <button class="btn btn--primary" :disabled="loading" @click="mergeAllExact">
            {{ loading ? 'Merging...' : 'Merge All' }}
          </button>
        </div>
        <p class="dedup-view__section-desc">
          These pairs matched on DOI or title (≥95% similarity) and can be safely merged. Click
          "Merge All" to merge them, or review each pair individually.
        </p>
        <div class="dedup-view__pairs">
          <div
            v-for="pair in result.exactDuplicates"
            :key="`exact-${pair.articleAId}-${pair.articleBId}`"
            class="dedup-view__exact-pair"
          >
            <div class="dedup-view__pair-meta">
              <span class="dedup-view__strategy">{{
                pair.strategy.replace(/([A-Z])/g, ' $1').trim()
              }}</span>
              <span class="dedup-view__similarity">{{ (pair.similarity * 100).toFixed(1) }}%</span>
            </div>
            <div class="dedup-view__pair-records">
              <div class="dedup-view__record">
                <p class="dedup-view__title">{{ pair.articleATitle }}</p>
                <p class="dedup-view__meta">{{ pair.articleAAuthors.join('; ') }}</p>
                <p class="dedup-view__meta">{{ pair.articleAYear ?? 'No year' }}</p>
              </div>
              <div class="dedup-view__record">
                <p class="dedup-view__title">{{ pair.articleBTitle }}</p>
                <p class="dedup-view__meta">{{ pair.articleBAuthors.join('; ') }}</p>
                <p class="dedup-view__meta">{{ pair.articleBYear ?? 'No year' }}</p>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- Section 2: Manual Review (Fuzzy Matches) -->
      <section v-if="result.fuzzyMatches.length > 0" class="dedup-view__section">
        <h2>Potential Duplicates ({{ result.fuzzyMatches.length }} remaining)</h2>
        <p class="dedup-view__section-desc">
          These pairs may be duplicates. Review each and choose which to keep.
        </p>
        <div class="dedup-view__pairs">
          <DedupPair
            v-for="pair in result.fuzzyMatches"
            :key="`fuzzy-${pair.articleAId}-${pair.articleBId}`"
            :pair="pair"
            @resolve="(r: DedupResolution) => onResolve(pair, r)"
          />
        </div>
      </section>

      <!-- All done -->
      <div
        v-if="result.exactDuplicates.length === 0 && result.fuzzyMatches.length === 0"
        class="dedup-view__done"
      >
        <h2>Deduplication Complete</h2>
        <p>
          <template v-if="mergedCount > 0"> {{ mergedCount }} exact duplicates merged. </template>
          <template v-if="resolvedCount > 0">
            {{ resolvedCount }} fuzzy matches resolved.
          </template>
          <template v-if="mergedCount === 0 && resolvedCount === 0">
            No duplicates found. All articles are unique.
          </template>
        </p>
      </div>
    </div>

    <div v-if="!result && !loading" class="dedup-view__empty">
      <p>No duplicate check has been run yet. Import articles first, then check for duplicates.</p>
      <button class="btn btn--primary" @click="checkDuplicates">Check for Duplicates</button>
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

  .dedup-view__pair-records {
    flex-direction: column;
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

.dedup-view__stat--done {
  background-color: var(--color-surface-container-low);
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

.dedup-view__section {
  margin-bottom: var(--space-6);
}

.dedup-view__section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-2);
}

.dedup-view__section-header h2 {
  margin: 0;
}

.dedup-view__section-desc {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  margin-bottom: var(--space-4);
}

.dedup-view__pairs {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-top: var(--space-4);
}

.dedup-view__exact-pair {
  border: 1px solid var(--color-border);
  border-left: 3px solid var(--color-primary);
  border-radius: var(--radius-default);
  padding: var(--space-3) var(--space-4);
}

.dedup-view__pair-meta {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  margin-bottom: var(--space-2);
}

.dedup-view__strategy {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  color: var(--color-primary);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-label);
}

.dedup-view__similarity {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  font-weight: var(--font-weight-semibold);
}

.dedup-view__pair-records {
  display: flex;
  gap: var(--space-3);
}

.dedup-view__record {
  flex: 1;
  padding: var(--space-2) var(--space-3);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-sm);
}

.dedup-view__title {
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-1);
}

.dedup-view__meta {
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
}

.dedup-view__done,
.dedup-view__empty {
  padding: var(--space-6);
  text-align: center;
  color: var(--color-on-surface-variant);
}

.dedup-view__empty .btn {
  margin-top: var(--space-3);
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

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
