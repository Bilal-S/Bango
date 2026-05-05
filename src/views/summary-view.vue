<script setup lang="ts">
import { useSummary } from '@/composables/use-summary';

const { summary, loading, error, generate } = useSummary();
</script>

<template>
  <div class="summary-view">
    <div class="summary-view__header">
      <h1>AI Summary</h1>
      <button class="btn btn--primary" :disabled="loading" @click="generate()">
        {{ loading ? 'Generating...' : 'Generate Summary' }}
      </button>
    </div>

    <div v-if="error" class="summary-view__error">{{ error }}</div>

    <div v-if="summary" class="summary-view__content">
      <section class="summary-section">
        <h2>Key Themes</h2>
        <p>{{ summary.keyThemes }}</p>
      </section>
      <section class="summary-section">
        <h2>Research Trends</h2>
        <p>{{ summary.researchTrends }}</p>
      </section>
      <section class="summary-section">
        <h2>Methodological Strengths</h2>
        <p>{{ summary.methodologicalStrengths }}</p>
      </section>
      <section class="summary-section">
        <h2>Common Weaknesses</h2>
        <p>{{ summary.commonWeaknesses }}</p>
      </section>
      <section class="summary-section">
        <h2>Gaps in Literature</h2>
        <p>{{ summary.gapsInLiterature }}</p>
      </section>
    </div>
  </div>
</template>

<style scoped>
.summary-view {
  padding: var(--space-6, 24px);
  max-width: 800px;
}
.summary-view__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: var(--space-6, 24px);
}
.summary-view__header h1 {
  font-size: var(--font-size-display, 24px);
  font-weight: var(--font-weight-semibold, 600);
  letter-spacing: var(--letter-spacing-display, -0.02em);
}
.summary-view__error {
  padding: var(--space-3, 12px);
  background-color: var(--color-error-container, #ffdad6);
  color: var(--color-error, #ba1a1a);
  border-radius: var(--radius-default, 0.25rem);
  margin-bottom: var(--space-4, 16px);
}
.summary-view__content {
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 16px);
}
.summary-section {
  border: 1px solid var(--color-border, #c7c4d8);
  border-radius: var(--radius-default, 0.25rem);
  padding: var(--space-4, 16px);
}
.summary-section h2 {
  font-size: var(--font-size-h2, 16px);
  margin-bottom: var(--space-2, 8px);
}
.summary-section p {
  font-size: var(--font-size-caption, 13px);
  line-height: 1.6;
}
.btn {
  padding: var(--space-2, 8px) var(--space-4, 16px);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
  font-weight: var(--font-weight-semibold, 600);
  cursor: pointer;
}
.btn--primary {
  background-color: var(--color-primary, #3525cd);
  color: var(--color-on-primary, #ffffff);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
