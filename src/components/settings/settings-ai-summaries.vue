<script setup lang="ts">
import { ref } from 'vue';

// AI summary preferences (persisted in localStorage; keys shared with the old
// Screening Preferences card so existing user prefs carry over unchanged).
const fullTextSummaries = ref(localStorage.getItem('bango-full-text-summaries') === 'true');

function toggleFullTextSummaries(): void {
  fullTextSummaries.value = !fullTextSummaries.value;
  localStorage.setItem('bango-full-text-summaries', String(fullTextSummaries.value));
}

const sectionSummaries = ref(localStorage.getItem('bango-section-summaries') === 'true');

function toggleSectionSummaries(): void {
  sectionSummaries.value = !sectionSummaries.value;
  localStorage.setItem('bango-section-summaries', String(sectionSummaries.value));
}
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">auto_awesome</span>
      AI Summaries
    </h2>
    <p class="settings-card__desc">
      Control how Bango generates AI summaries for articles with attached full text.
    </p>

    <div class="settings-card__toggle-row">
      <label class="settings-card__toggle-label">
        <span>Auto Generate Summaries</span>
        <span class="settings-card__toggle-hint">
          When enabled, articles with uploaded full text are automatically summarized using the
          configured LLM as soon as the attachment completes. The manual
          <span class="inline-icon material-symbols-outlined">auto_awesome</span>
          <strong>Generate Summary</strong> button on each article works regardless of this setting.
        </span>
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

    <div class="settings-card__toggle-row" style="margin-top: 1rem">
      <label class="settings-card__toggle-label">
        <span>Section Summaries</span>
        <span class="settings-card__toggle-hint">
          When enabled, AI summaries also include per-section breakdowns (Methods, Results,
          Discussion). Generates richer output in the same LLM call. Only applies when a summary is
          generated (automatically or via the manual button).
        </span>
      </label>
      <button
        class="settings-card__switch"
        :class="{ 'settings-card__switch--on': sectionSummaries }"
        role="switch"
        :aria-checked="sectionSummaries"
        @click="toggleSectionSummaries"
      >
        <span class="settings-card__switch-thumb" />
      </button>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

/* Inline Material Symbols icon within hint prose. Sized to sit on the text
   baseline so the user can visually match it to the article detail button. */
.inline-icon {
  font-size: 15px;
  vertical-align: middle;
  position: relative;
  top: -1px;
  color: var(--color-primary, #3525cd);
  user-select: none;
}
</style>
