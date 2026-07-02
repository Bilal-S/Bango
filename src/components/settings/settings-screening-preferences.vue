<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { ScreeningMode } from '@/types';
import { isTauri } from '@/composables/use-tauri-command';

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

// Section Summaries preference (off by default; independent of Full Text Summaries)
const sectionSummaries = ref(localStorage.getItem('bango-section-summaries') === 'true');

function toggleSectionSummaries(): void {
  sectionSummaries.value = !sectionSummaries.value;
  localStorage.setItem('bango-section-summaries', String(sectionSummaries.value));
}

// ── Tier 3: Screening Mode (abstract | enhanced | two_stage) ────────────────
// Persisted in app_settings (not localStorage) so it survives across devices
// via project backup. Enhanced / Two-stage are disabled until at least one
// full-text article exists.
const screeningMode = ref<ScreeningMode>('abstract');
const fullTextArticleCount = ref(0);
const modeLoading = ref(false);
const modeError = ref<string | null>(null);

const modes: Array<{
  value: ScreeningMode;
  label: string;
  description: string;
}> = [
  {
    value: 'abstract',
    label: 'Abstract only',
    description: 'Default. Screens on the abstract alone (~1x token cost).',
  },
  {
    value: 'enhanced',
    label: 'Enhanced',
    description:
      'Sends abstract + the top-2 criteria-matched Methods/Results chunks (~5x cost). Best when full text is attached.',
  },
  {
    value: 'two_stage',
    label: 'Two-stage',
    description:
      'Abstract first; only borderline articles (confidence 0.4-0.7) get a second full-text pass (~1.5x effective cost). Most cost-efficient.',
  },
];

const advancedDisabled = (mode: ScreeningMode): boolean =>
  (mode === 'enhanced' || mode === 'two_stage') && fullTextArticleCount.value < 1;

const advancedDisabledTooltip = (mode: ScreeningMode): string =>
  advancedDisabled(mode) ? 'Attach full text to at least one article to enable this mode.' : '';

async function loadScreeningMode(): Promise<void> {
  if (!isTauri()) {
    // Non-Tauri (unit test) mode: keep the default.
    return;
  }
  modeLoading.value = true;
  modeError.value = null;
  try {
    const [mode, count] = await Promise.all([
      invoke<ScreeningMode>('get_screening_mode'),
      invoke<number>('get_full_text_article_count'),
    ]);
    screeningMode.value = mode;
    fullTextArticleCount.value = count;
  } catch (e: unknown) {
    modeError.value = e instanceof Error ? e.message : String(e);
  } finally {
    modeLoading.value = false;
  }
}

async function selectMode(mode: ScreeningMode): Promise<void> {
  if (advancedDisabled(mode) || mode === screeningMode.value) return;
  screeningMode.value = mode;
  if (!isTauri()) return;
  try {
    await invoke('set_screening_mode', { mode });
  } catch (e: unknown) {
    modeError.value = e instanceof Error ? e.message : String(e);
    // Revert on error.
    await loadScreeningMode();
  }
}

onMounted(() => {
  void loadScreeningMode();
});
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">navigate_next</span>
      Screening Preferences
    </h2>
    <p class="settings-card__desc">Configure behavior when screening articles.</p>

    <!-- Tier 3: Screening Mode dropdown -->
    <div class="settings-card__group">
      <div class="settings-card__group-label">Screening Mode</div>
      <select
        class="mode-select"
        :value="screeningMode"
        :disabled="modeLoading"
        :title="advancedDisabled(screeningMode) ? advancedDisabledTooltip(screeningMode) : ''"
        @change="selectMode(($event.target as HTMLSelectElement).value as ScreeningMode)"
      >
        <option
          v-for="m in modes"
          :key="m.value"
          :value="m.value"
          :disabled="advancedDisabled(m.value)"
        >
          {{ m.label }}
        </option>
      </select>
      <p v-if="screeningMode !== 'abstract'" class="mode-select__desc">
        {{ modes.find((m) => m.value === screeningMode)?.description }}
      </p>
      <p v-if="modeError" class="settings-card__status mode-error">{{ modeError }}</p>
      <p v-if="fullTextArticleCount < 1" class="settings-card__hint">
        Enhanced and Two-stage modes require at least one article with attached full text.
      </p>
    </div>

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
    <div class="settings-card__toggle-row" style="margin-top: 1rem">
      <label class="settings-card__toggle-label">
        <span>Section Summaries</span>
        <span class="settings-card__toggle-hint"
          >When enabled, AI summaries also include per-section breakdowns (Methods, Results,
          Discussion). Generates more detailed output per article in the same LLM call.</span
        >
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

/* ── Tier 3: Mode dropdown ── */
.settings-card__group {
  margin-bottom: 1.5rem;
}

.settings-card__group-label {
  font-size: 14px;
  font-weight: 500;
  color: var(--color-on-surface, #1b1b24);
  margin-bottom: 0.25rem;
}

.mode-select {
  width: 100%;
  max-width: 320px;
  padding: 0.5rem 0.75rem;
  font-size: 14px;
  font-family: inherit;
  color: var(--color-on-surface, #1b1b24);
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: var(--radius-default, 0.5rem);
  cursor: pointer;
  transition: border-color 0.15s ease;
}

.mode-select:hover:not(:disabled) {
  border-color: var(--color-primary, #3525cd);
}

.mode-select:focus {
  outline: none;
  border-color: var(--color-primary, #3525cd);
  box-shadow: 0 0 0 3px var(--color-primary, #3525cd);
}

.mode-select:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.mode-select__desc {
  font-size: 12px;
  color: var(--color-on-surface-variant, #464555);
  line-height: 1.4;
  margin-top: 0.375rem;
  max-width: 480px;
}

.mode-error {
  color: #991b1b;
  background-color: #fef2f2;
}
</style>
