<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { isTauri } from '@/composables/use-tauri-command';

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

// ── Auto Translate (experimental) ─────────────────────────────────────────
// Unlike the two toggles above, this is persisted in the database
// (`app_settings.auto_translate`) so backend processing stages can read it
// directly. Defaults to enabled (true) when the key is absent.
const autoTranslate = ref(true);
const autoTranslateError = ref<string | null>(null);

async function loadAutoTranslate(): Promise<void> {
  if (!isTauri()) {
    // Non-Tauri (unit test) mode: keep the default (enabled).
    return;
  }
  try {
    autoTranslate.value = await invoke<boolean>('get_auto_translate');
  } catch (e: unknown) {
    // Non-fatal: keep the default on read failure.
    autoTranslate.value = true;
  }
}

async function toggleAutoTranslate(): Promise<void> {
  const next = !autoTranslate.value;
  autoTranslate.value = next;
  autoTranslateError.value = null;
  if (!isTauri()) {
    return;
  }
  try {
    await invoke('set_auto_translate', { enabled: next });
  } catch (e: unknown) {
    autoTranslateError.value = e instanceof Error ? e.message : String(e);
    // Revert on error.
    autoTranslate.value = !next;
    await loadAutoTranslate();
  }
}

onMounted(() => {
  void loadAutoTranslate();
});
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

    <div class="settings-card__toggle-row" style="margin-top: 1rem">
      <label class="settings-card__toggle-label">
        <span>
          Auto Translate
          <span class="badge--experimental">Experimental</span>
        </span>
        <span class="settings-card__toggle-hint">
          When enabled, articles written in other languages are translated to English during AI
          processing (screening and summaries). This is an experimental feature and may be refined
          in future releases.
        </span>
      </label>
      <button
        class="settings-card__switch"
        :class="{ 'settings-card__switch--on': autoTranslate }"
        role="switch"
        :aria-checked="autoTranslate"
        @click="toggleAutoTranslate"
      >
        <span class="settings-card__switch-thumb" />
      </button>
    </div>
    <p v-if="autoTranslateError" class="settings-card__status settings-card__status--err">
      {{ autoTranslateError }}
    </p>
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

/* Experimental pill badge used inline next to the Auto Translate label. */
.badge--experimental {
  display: inline-block;
  margin-left: 0.375rem;
  padding: 0.0625rem 0.5rem;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--color-primary, #3525cd);
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: var(--radius-pill, 9999px);
  vertical-align: middle;
  user-select: none;
}
</style>
