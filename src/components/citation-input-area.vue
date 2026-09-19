<script lang="ts">
/** Status-filter checkbox state. Duplicates are always excluded. */
export interface CitationStatusFlags {
  working: boolean;
  included: boolean;
  rejected: boolean;
}
</script>

<script setup lang="ts">
/**
 * Citation Finder input area: replaces the article-context pills + chat bar
 * while Citation Finder mode is active. Holds the disabled-provider banner,
 * the coverage / first-run notice, the style dropdown, status checkboxes,
 * scope toggle, prose textarea, the Find Citations button, and the live
 * progress + Cancel UI. Presentational: every state change is emitted.
 */
import type {
  CitationFinderMode,
  CitationFinderProgress,
  CitationFinderReadiness,
  CitationStyle,
} from '@/types/citation-finder';

/** The shared 5-style citation list (matches the AI Summary list). */
const CITATION_STYLE_OPTIONS: CitationStyle[] = ['APA', 'MLA', 'Chicago', 'IEEE', 'AMA'];

defineProps<{
  /** Readiness payload (drives the coverage notice). */
  readiness: CitationFinderReadiness | null;
  /** Toggle state (drives the disabled-provider banner). */
  toggleState: 'enabled' | 'unknown' | 'disabled' | 'hidden';
  /** Selected citation style (frozen per bubble at submit). */
  styleValue: CitationStyle;
  /** Whole-block vs per-statement scope. */
  mode: CitationFinderMode;
  /** Status-filter checkboxes. */
  statuses: CitationStatusFlags;
  /** Prose draft (held in the chat store). */
  draft: string;
  /** Live progress payload; non-null replaces the Find button. */
  progress: CitationFinderProgress | null;
  /** True while a chat/citation operation is in flight. */
  loading: boolean;
  /** True while a cancel is draining. */
  cancelling: boolean;
}>();

const emit = defineEmits<{
  'update:style': [style: CitationStyle];
  'update:mode': [mode: CitationFinderMode];
  'update:statuses': [statuses: CitationStatusFlags];
  'update:draft': [draft: string];
  send: [];
  cancel: [];
  close: [];
  openSettings: [];
}>();

function onStyleChange(event: Event) {
  emit('update:style', (event.target as HTMLSelectElement).value as CitationStyle);
}

function onStatusToggle(
  statuses: CitationStatusFlags,
  key: keyof CitationStatusFlags,
  event: Event
) {
  const checked = (event.target as HTMLInputElement).checked;
  emit('update:statuses', { ...statuses, [key]: checked });
}
</script>

<template>
  <div class="citation-input-area">
    <!-- Disabled-provider banner: shown when the user managed to enter
         citation mode but the current provider does not support
         embeddings. Mirrors the wiki-banner pattern. -->
    <div v-if="toggleState === 'disabled'" class="citation-disabled-banner">
      <span class="material-symbols-outlined text-[16px]">block</span>
      <span class="citation-disabled-banner__text">
        Current provider does not support Citation Finder embeddings. Switch to an embedding-capable
        provider in Settings.
      </span>
      <button
        class="citation-disabled-banner__btn"
        title="Open Settings"
        @click="emit('openSettings')"
      >
        Open Settings
      </button>
    </div>

    <!-- Coverage / first-run notice: surfaces the readiness payload's
         coverage so the user knows whether the first search will trigger a
         one-time embedding-generation pass (Phase B) and how many articles
         are in scope. Hidden when there are no articles in the selected
         statuses or when coverage is already complete. -->
    <div
      v-if="readiness && readiness.totalArticles > 0 && readiness.coveragePct < 100 && !progress"
      class="citation-coverage-notice"
    >
      <span class="material-symbols-outlined text-[14px]">database</span>
      <span>
        First run will prepare embeddings for
        {{ readiness.totalArticles }} article(s) - this may take several minutes. Subsequent
        searches are fast.
      </span>
    </div>

    <!-- Single control row: Citation Style dropdown, status checkboxes,
         Mode segmented toggle, close (X). Everything sits at the same level
         so there is no extra whitespace; the close button is pushed to the
         right edge with margin-left:auto. -->
    <div class="citation-input-area__row">
      <label class="citation-input-area__field">
        <span class="citation-input-area__label">Citation Style</span>
        <select :value="styleValue" class="citation-input-area__select" @change="onStyleChange">
          <option v-for="s in CITATION_STYLE_OPTIONS" :key="s" :value="s">{{ s }}</option>
        </select>
      </label>

      <!-- Status checkboxes. Duplicate is always excluded. -->
      <div class="citation-input-area__field" role="group" aria-label="Status filter">
        <span class="citation-input-area__label">Articles to Search</span>
        <div class="citation-input-area__statuses">
          <label class="citation-input-area__checkbox">
            <input
              :checked="statuses.working"
              type="checkbox"
              @change="onStatusToggle(statuses, 'working', $event)"
            />
            <span>Working</span>
          </label>
          <label class="citation-input-area__checkbox">
            <input
              :checked="statuses.included"
              type="checkbox"
              @change="onStatusToggle(statuses, 'included', $event)"
            />
            <span>Included</span>
          </label>
          <label class="citation-input-area__checkbox">
            <input
              :checked="statuses.rejected"
              type="checkbox"
              @change="onStatusToggle(statuses, 'rejected', $event)"
            />
            <span>Rejected</span>
          </label>
        </div>
        <span class="citation-input-area__statuses-hint">Duplicates always excluded</span>
      </div>

      <!-- Mode toggle with a "SCOPE" header matching Citation Style. -->
      <div class="citation-input-area__field" role="group" aria-label="Citation Finder mode">
        <span class="citation-input-area__label">Scope</span>
        <div class="citation-input-area__mode">
          <button
            type="button"
            class="citation-input-area__mode-btn"
            :class="{ 'citation-input-area__mode-btn--active': mode === 'whole_block' }"
            @click="emit('update:mode', 'whole_block')"
          >
            Whole Block
          </button>
          <button
            type="button"
            class="citation-input-area__mode-btn"
            :class="{ 'citation-input-area__mode-btn--active': mode === 'per_statement' }"
            @click="emit('update:mode', 'per_statement')"
          >
            Per Statement
          </button>
        </div>
      </div>
      <button
        type="button"
        class="citation-input-area__close"
        title="Close Citation Finder"
        @click="emit('close')"
      >
        <span class="material-symbols-outlined text-[18px]">close</span>
      </button>
    </div>

    <!-- Prose textarea + (Find Citations button OR live progress). While a
         search is running, the progress indicator replaces the Find
         Citations button in place - the textarea stays visible so the user
         can draft the next search. -->
    <div class="citation-input-area__prose-row">
      <textarea
        :value="draft"
        class="citation-input-area__textarea"
        placeholder="Paste the text you want to find citations for..."
        rows="4"
        @input="emit('update:draft', ($event.target as HTMLTextAreaElement).value)"
        @keydown.enter.ctrl="emit('send')"
        @keydown.enter.meta="emit('send')"
      ></textarea>

      <!-- Idle: Find Citations button -->
      <button
        v-if="!progress"
        type="button"
        class="citation-input-area__find-btn"
        :disabled="!draft.trim() || loading"
        @click="emit('send')"
      >
        <span class="material-symbols-outlined text-[18px]">search</span>
        Find Citations
      </button>

      <!-- Running: compact progress replaces the button -->
      <div v-else class="citation-progress citation-progress--inline">
        <div class="citation-progress__header">
          <span class="citation-progress__message">{{ progress.message }}</span>
          <button
            type="button"
            class="citation-progress__cancel"
            :disabled="cancelling"
            @click="emit('cancel')"
          >
            <span v-if="cancelling" class="citation-progress__cancel-spinner"></span>
            <span v-else class="material-symbols-outlined text-[14px]">cancel</span>
            {{ cancelling ? 'Cancelling…' : 'Cancel' }}
          </button>
        </div>
        <div class="citation-progress__bar-track">
          <div
            class="citation-progress__bar-fill"
            :style="{
              width:
                (progress.phase === 'preparing_embeddings' ? progress.overallPercent : 100) + '%',
            }"
            :class="{
              'citation-progress__bar-fill--indeterminate': progress.phase === 'searching',
            }"
          ></div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.citation-input-area {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  margin-bottom: 0.75rem;
}

/* Close (X) button - exits citation mode. Pushed to the right edge with
   margin-left:auto. Aligned to center so it lines up with the row's other
   controls regardless of label height. */
.citation-input-area__close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 1.75rem;
  height: 1.75rem;
  margin-left: auto;
  align-self: center;
  border-radius: 0.375rem;
  border: none;
  background: transparent;
  color: rgb(100 116 139); /* slate-500 */
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.citation-input-area__close:hover {
  background-color: rgb(241 245 249); /* slate-100 */
  color: rgb(15 23 42); /* slate-900 */
}

/* Inline variant of the progress block: constrains the width so it occupies
   the Find Citations button's column (instead of spanning the full row). */
.citation-progress--inline {
  flex-shrink: 0;
  min-width: 9rem;
  max-width: 12rem;
  justify-content: center;
}

.citation-input-area__row {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.citation-input-area__field {
  display: flex;
  flex-direction: column;
  gap: 0.1875rem;
}

.citation-input-area__label {
  font-size: 0.625rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: rgb(100 116 139); /* slate-500 */
}

.citation-input-area__select {
  padding: 0.3125rem 0.5rem;
  border: 1px solid rgb(203 213 225); /* slate-300 */
  border-radius: 0.375rem;
  background: #fff;
  font-size: 0.75rem;
  color: rgb(15 23 42); /* slate-900 */
  cursor: pointer;
}

.citation-input-area__mode {
  display: inline-flex;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.375rem;
  overflow: hidden;
}

.citation-input-area__mode-btn {
  padding: 0.3125rem 0.625rem;
  border: none;
  background: #fff;
  font-size: 0.6875rem;
  font-weight: 600;
  color: rgb(71 85 105); /* slate-600 */
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.citation-input-area__mode-btn:not(.citation-input-area__mode-btn--active):hover {
  background: rgb(241 245 249); /* slate-100 */
}

.citation-input-area__mode-btn--active {
  background: rgb(99 102 241); /* indigo-600 */
  color: #fff;
}

.citation-input-area__statuses {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 0.625rem;
}

.citation-input-area__checkbox {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.6875rem;
  color: rgb(71 85 105); /* slate-600 */
  cursor: pointer;
}

.citation-input-area__checkbox input {
  accent-color: rgb(99 102 241); /* indigo-600 */
}

.citation-input-area__statuses-hint {
  font-size: 0.625rem;
  color: rgb(148 163 184); /* slate-400 */
  font-style: italic;
}

.citation-input-area__prose-row {
  display: flex;
  gap: 0.5rem;
  align-items: stretch;
}

.citation-input-area__textarea {
  flex: 1;
  padding: 0.5rem 0.625rem;
  border: 1px solid rgb(203 213 225); /* slate-300 */
  border-radius: 0.5rem;
  font-size: 0.8rem;
  line-height: 1.4;
  color: rgb(15 23 42);
  resize: vertical;
  min-height: 4.5rem;
  font-family: inherit;
}

.citation-input-area__textarea:focus {
  outline: none;
  border-color: rgb(99 102 241); /* indigo-600 */
  box-shadow: 0 0 0 2px rgb(99 102 241 / 0.2);
}

.citation-input-area__find-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.5rem 0.875rem;
  border: none;
  border-radius: 0.5rem;
  background: rgb(99 102 241); /* indigo-600 */
  color: #fff;
  font-size: 0.75rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.15s;
  flex-shrink: 0;
}

.citation-input-area__find-btn:hover:not(:disabled) {
  background: rgb(79 70 229); /* indigo-700 */
}

.citation-input-area__find-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

/* Live progress bar + Cancel button. */
.citation-progress {
  display: flex;
  flex-direction: column;
  gap: 0.3125rem;
  padding: 0.5rem 0.625rem;
  background: rgb(248 250 252); /* slate-50 */
  border: 1px solid rgb(226 232 240); /* slate-200 */
  border-radius: 0.375rem;
}

.citation-progress__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
}

.citation-progress__message {
  font-size: 0.6875rem;
  font-weight: 500;
  color: rgb(71 85 105); /* slate-600 */
}

.citation-progress__cancel {
  display: inline-flex;
  align-items: center;
  gap: 0.1875rem;
  padding: 0.1875rem 0.4375rem;
  border: 1px solid rgb(254 202 202); /* red-200 */
  border-radius: 0.25rem;
  background: #fff;
  color: rgb(220 38 38); /* red-600 */
  font-size: 0.625rem;
  font-weight: 600;
  cursor: pointer;
  transition: background-color 0.15s;
}

.citation-progress__cancel:hover:not(:disabled) {
  background: rgb(254 226 226); /* red-100 */
}

.citation-progress__cancel:disabled {
  opacity: 0.6;
  cursor: default;
}

/* Small spinner shown next to "Cancelling…" while the backend drains the
 * in-flight LLM call + emits the terminal `citation:error`. */
.citation-progress__cancel-spinner {
  display: inline-block;
  width: 0.75rem;
  height: 0.75rem;
  border: 1.5px solid rgb(220 38 38 / 0.3); /* red-600 @ 30% */
  border-top-color: rgb(220 38 38); /* red-600 */
  border-radius: 9999px;
  animation: citation-cancel-spin 0.7s linear infinite;
}

@keyframes citation-cancel-spin {
  to {
    transform: rotate(360deg);
  }
}

.citation-progress__bar-track {
  width: 100%;
  height: 0.25rem;
  background: rgb(226 232 240); /* slate-200 */
  border-radius: 9999px;
  overflow: hidden;
}

.citation-progress__bar-fill {
  height: 100%;
  background: rgb(99 102 241); /* indigo-600 */
  border-radius: 9999px;
  transition: width 0.2s ease;
}

.citation-progress__bar-fill--indeterminate {
  animation: citation-progress-indeterminate 1.4s ease-in-out infinite;
}

@keyframes citation-progress-indeterminate {
  0% {
    transform: translateX(-100%);
  }
  50% {
    transform: translateX(0%);
  }
  100% {
    transform: translateX(100%);
  }
}

.citation-disabled-banner {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  border-radius: 0.5rem;
  background-color: rgb(254 243 199); /* amber-100 */
  border: 1px solid rgb(252 211 77); /* amber-300 */
  color: rgb(120 53 15); /* amber-900 */
}

.citation-disabled-banner__text {
  flex: 1;
  font-size: 0.72rem;
  font-weight: 600;
}

.citation-disabled-banner__btn {
  padding: 0.1875rem 0.5rem;
  border-radius: 0.25rem;
  border: 1px solid rgb(252 211 77); /* amber-300 */
  background: #fff;
  color: rgb(180 83 9); /* amber-700 */
  font-size: 0.65rem;
  font-weight: 700;
  cursor: pointer;
  transition: background-color 0.15s;
}

.citation-disabled-banner__btn:hover {
  background: rgb(254 249 195); /* amber-50 */
}

/* Coverage / first-run notice. Muted slate chrome (not a warning - just an
   FYI that the first search will trigger a one-time embedding pass). */
.citation-coverage-notice {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.625rem;
  border-radius: 0.375rem;
  background-color: rgb(241 245 249); /* slate-100 */
  border: 1px solid rgb(226 232 240); /* slate-200 */
  color: rgb(71 85 105); /* slate-600 */
  font-size: 0.6875rem;
}
</style>
