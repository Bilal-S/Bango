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

const modes: Array<{ value: ScreeningMode; label: string }> = [
  { value: 'abstract', label: 'Abstract only' },
  { value: 'enhanced', label: 'Enhanced' },
  { value: 'two_stage', label: 'Two-stage' },
];

// Two-stage borderline band (integer percent), persisted in app_settings as f64
// fractions (defaults 0.4 / 0.7). Shown only when Two-stage mode is selected,
// inline with the mode select. Displayed in percent to match the percent
// confidence shown in the article list and AI decision card.
interface TwoStageThresholds {
  lowPct: number;
  highPct: number;
}
const twoStageLowPct = ref(40);
const twoStageHighPct = ref(70);
const thresholdError = ref<string | null>(null);

const twoStageDescription = computed(
  () =>
    `Abstract first; only borderline articles (confidence ${twoStageLowPct.value}-${twoStageHighPct.value}%) get a second full-text pass (~1.5x effective cost). Most cost-efficient.`
);

const modeDescription = computed(() => {
  switch (screeningMode.value) {
    case 'abstract':
      return 'Default. Screens on the abstract alone (~1x token cost).';
    case 'enhanced':
      return 'Sends abstract + the top-2 criteria-matched Methods/Results chunks (~5x cost). Best when full text is attached.';
    case 'two_stage':
      return twoStageDescription.value;
    default:
      return '';
  }
});

// Client-side validity for the two-stage band: both 0-100 and strict low < high.
const thresholdValid = computed(
  () =>
    twoStageLowPct.value >= 0 &&
    twoStageLowPct.value <= 100 &&
    twoStageHighPct.value >= 0 &&
    twoStageHighPct.value <= 100 &&
    twoStageLowPct.value < twoStageHighPct.value
);

// True when the active mode can use full-text evidence but no article has FT.
const advancedModeWithoutFullText = computed(
  () => screeningMode.value !== 'abstract' && fullTextArticleCount.value < 1
);

function clampPct(v: number): number {
  if (Number.isNaN(v)) return 0;
  return Math.max(0, Math.min(100, Math.round(v)));
}

async function loadScreeningPreferences(): Promise<void> {
  if (!isTauri()) {
    // Non-Tauri (unit test) mode: keep the defaults.
    return;
  }
  modeLoading.value = true;
  modeError.value = null;
  try {
    const [mode, count, thresholds] = await Promise.all([
      invoke<ScreeningMode>('get_screening_mode'),
      invoke<number>('get_full_text_article_count'),
      invoke<TwoStageThresholds>('get_two_stage_thresholds'),
    ]);
    screeningMode.value = mode;
    fullTextArticleCount.value = count;
    twoStageLowPct.value = thresholds.lowPct;
    twoStageHighPct.value = thresholds.highPct;
    thresholdError.value = null;
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
    await loadScreeningPreferences();
  }
}

async function saveThresholds(): Promise<void> {
  if (!isTauri()) return;
  // Normalize + clamp locally before sending.
  const low = clampPct(twoStageLowPct.value);
  const high = clampPct(twoStageHighPct.value);
  twoStageLowPct.value = low;
  twoStageHighPct.value = high;
  if (!(low < high)) {
    thresholdError.value = 'Lower threshold must be less than the upper threshold.';
    return;
  }
  thresholdError.value = null;
  try {
    const result = await invoke<TwoStageThresholds>('set_two_stage_thresholds', {
      lowPct: low,
      highPct: high,
    });
    twoStageLowPct.value = result.lowPct;
    twoStageHighPct.value = result.highPct;
  } catch (e: unknown) {
    thresholdError.value = e instanceof Error ? e.message : String(e);
    // Revert to the persisted state.
    await loadScreeningPreferences();
  }
}

onMounted(() => {
  void loadScreeningPreferences();
});
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">navigate_next</span>
      Screening Preferences
    </h2>
    <p class="settings-card__desc">Configure behavior when screening articles.</p>

    <!-- Tier 3: Screening Mode dropdown + Two-stage borderline band -->
    <div class="settings-card__group">
      <div class="settings-card__group-label">Screening Mode</div>
      <div class="mode-row">
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
        <!-- Two-stage borderline band: only shown for Two-stage mode. Sits on
             the same row as the mode select, with a label above each input. -->
        <div v-if="screeningMode === 'two_stage'" class="threshold-row">
          <div class="threshold-field">
            <label class="threshold-field__label" for="two-stage-low">Lower %</label>
            <div class="threshold-field__input">
              <input
                id="two-stage-low"
                v-model.number="twoStageLowPct"
                type="number"
                class="threshold-input"
                min="0"
                max="100"
                :aria-invalid="!thresholdValid"
                :disabled="modeLoading"
                @change="saveThresholds()"
              />
              <span class="threshold-field__suffix">%</span>
            </div>
          </div>
          <div class="threshold-field">
            <label class="threshold-field__label" for="two-stage-high">Upper %</label>
            <div class="threshold-field__input">
              <input
                id="two-stage-high"
                v-model.number="twoStageHighPct"
                type="number"
                class="threshold-input"
                min="0"
                max="100"
                :aria-invalid="!thresholdValid"
                :disabled="modeLoading"
                @change="saveThresholds()"
              />
              <span class="threshold-field__suffix">%</span>
            </div>
          </div>
        </div>
      </div>
      <p class="mode-select__desc">{{ modeDescription }}</p>
      <p v-if="thresholdError" class="settings-card__status mode-error">{{ thresholdError }}</p>
      <p v-else-if="modeError" class="settings-card__status mode-error">{{ modeError }}</p>
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

/* ── Two-stage borderline band (inline with the mode select) ── */
.mode-row {
  display: flex;
  align-items: flex-end;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.threshold-row {
  display: flex;
  align-items: flex-end;
  gap: 0.75rem;
}

.threshold-field {
  display: flex;
  flex-direction: column;
}

.threshold-field__label {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-on-surface-variant, #464555);
  margin-bottom: 0.25rem;
}

.threshold-field__input {
  position: relative;
  display: inline-flex;
  align-items: center;
}

.threshold-input {
  width: 5.5rem;
  padding: 0.5rem 1.75rem 0.5rem 0.75rem;
  font-size: 14px;
  font-family: inherit;
  color: var(--color-on-surface, #1b1b24);
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: var(--radius-default, 0.5rem);
  transition: border-color 0.15s ease;
}

.threshold-input:hover:not(:disabled) {
  border-color: var(--color-primary, #3525cd);
}

.threshold-input:focus {
  outline: none;
  border-color: var(--color-primary, #3525cd);
  box-shadow: 0 0 0 3px var(--color-primary, #3525cd);
}

.threshold-input:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.threshold-field__suffix {
  position: absolute;
  right: 0.625rem;
  font-size: 13px;
  color: var(--color-on-surface-variant, #464555);
  pointer-events: none;
}
</style>
