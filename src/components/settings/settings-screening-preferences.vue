<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
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

/* Screening Mode: persisted in app_settings (not localStorage) so it survives
   project backup. All three modes always selectable; engine applies Enhanced /
   Two-stage evidence only to articles with full text and falls back silently. */
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

// True when the active mode can use full-text evidence but no article has FT.
const advancedModeWithoutFullText = computed(
  () => screeningMode.value !== 'abstract' && fullTextArticleCount.value < 1
);

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
  if (mode === screeningMode.value) return;
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
        @change="selectMode(($event.target as HTMLSelectElement).value as ScreeningMode)"
      >
        <option v-for="m in modes" :key="m.value" :value="m.value">
          {{ m.label }}
        </option>
      </select>
      <p v-if="screeningMode !== 'abstract'" class="mode-select__desc">
        {{ modes.find((m) => m.value === screeningMode)?.description }}
      </p>
      <p v-if="modeError" class="settings-card__status mode-error">{{ modeError }}</p>
      <!-- Condition + fallback notice. Modes are always selectable; the engine
           falls back to abstract-only screening per article until full text is
           attached. -->
      <p v-if="advancedModeWithoutFullText" class="mode-select__fallback">
        No articles have full text attached yet. This mode will fall back to abstract-only screening
        until at least one article has full text.
      </p>
      <p
        v-else-if="screeningMode !== 'abstract' && fullTextArticleCount > 0"
        class="mode-select__active"
      >
        {{ fullTextArticleCount }} article(s) have full text attached - evidence retrieval is
        active.
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

.mode-select__fallback {
  font-size: 12px;
  color: #92400e;
  background-color: #fef3c7;
  border: 1px solid #fde68a;
  border-radius: var(--radius-default, 0.375rem);
  padding: 0.5rem 0.625rem;
  line-height: 1.4;
  margin-top: 0.375rem;
  max-width: 480px;
}

.mode-select__active {
  font-size: 12px;
  color: #065f46;
  background-color: #d1fae5;
  border: 1px solid #a7f3d0;
  border-radius: var(--radius-default, 0.375rem);
  padding: 0.5rem 0.625rem;
  line-height: 1.4;
  margin-top: 0.375rem;
  max-width: 480px;
}
</style>
