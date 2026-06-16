<script setup lang="ts">
import { ref } from 'vue';

// Screening preferences (persisted in localStorage)
const autoNavigateAfterDecision = ref(
  localStorage.getItem('bango-auto-navigate-after-decision') !== 'false'
);

function toggleAutoNavigate(): void {
  autoNavigateAfterDecision.value = !autoNavigateAfterDecision.value;
  localStorage.setItem(
    'bango-auto-navigate-after-decision',
    String(autoNavigateAfterDecision.value)
  );
}

// Full Text Summaries preference (off by default)
const fullTextSummaries = ref(localStorage.getItem('bango-full-text-summaries') === 'true');

function toggleFullTextSummaries(): void {
  fullTextSummaries.value = !fullTextSummaries.value;
  localStorage.setItem('bango-full-text-summaries', String(fullTextSummaries.value));
}
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">navigate_next</span>
      Screening Preferences
    </h2>
    <p class="settings-card__desc">Configure behavior when screening articles.</p>
    <div class="settings-card__toggle-row">
      <label class="settings-card__toggle-label">
        <span>Auto-navigate to next article after decision</span>
        <span class="settings-card__toggle-hint"
          >When enabled, automatically advances to the next article after including or
          rejecting.</span
        >
      </label>
      <button
        class="settings-card__switch"
        :class="{ 'settings-card__switch--on': autoNavigateAfterDecision }"
        role="switch"
        :aria-checked="autoNavigateAfterDecision"
        @click="toggleAutoNavigate"
      >
        <span class="settings-card__switch-thumb" />
      </button>
    </div>
    <div class="settings-card__toggle-row" style="margin-top: 1rem">
      <label class="settings-card__toggle-label">
        <span>Full Text Summaries</span>
        <span class="settings-card__toggle-hint"
          >Auto-summarize full text when possible. When enabled, articles with uploaded full text
          will be automatically summarized using the configured LLM.</span
        >
      </label>
      <button
        class="settings-card__switch"
        :class="{ 'settings-card__switch--on': fullTextSummaries }"
        role="switch"
        :aria-checked="fullTextSummaries"
        @click="toggleFullTextSummaries"
      >
        <span class="settings-card__switch-thumb" />
      </button>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';
</style>
