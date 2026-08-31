<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { marked } from 'marked';
import { useSummary, type CitationStyle } from '@/composables/use-summary';
import { useGapAnalysis } from '@/composables/use-gap-analysis';
import { useArticlesStore } from '@/stores/articles';
import { useCriteriaStore } from '@/stores/criteria';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { useFeatureFlags } from '@/composables/use-feature-flags';
import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';

const {
  summaryText,
  loading: summaryLoading,
  error: summaryError,
  citationStyle,
  additionalInstructions: reviewInstructions,
  targetWordCount: reviewTargetWords,
  loadSaved: loadSavedSummary,
  generate: generateSummary,
  formatGeneratedAt: formatSummaryGeneratedAt,
} = useSummary();

const {
  gapText,
  loading: gapLoading,
  error: gapError,
  additionalInstructions: gapInstructions,
  targetWordCount: gapTargetWords,
  loadSaved: loadSavedGap,
  generate: generateGap,
  formatGeneratedAt: formatGapGeneratedAt,
} = useGapAnalysis();

/* Canonical premium gate (`useFeatureFlags` wraps the `get_app_flags` IPC).
 * The two per-report guidance cards below render only for premium users. */
const { isPremium } = useFeatureFlags();

const articlesStore = useArticlesStore();
const criteriaStore = useCriteriaStore();

/** Output mode: Literature Review or Research Gaps. Default is review. Set
 *  implicitly by which generate button is clicked. */
const mode = ref<'review' | 'gaps'>('review');

const copied = ref(false);

/** Premium guidance cards (one per report). Collapsed by default; the toggle
 *  state is view-local UI state, while the values live in the composables'
 *  module singletons so they survive navigation. */
const showReviewGuidance = ref(false);
const showGapGuidance = ref(false);

/** Parse a word-count input into a positive integer, or null when the field is
 *  blank / zero / invalid (null = no length constraint in the prompt). Accepts
 *  both string and number: `v-model` on `<input type="number">` auto-casts. */
function parseTargetWords(value: string | number): number | null {
  const n = Number(value);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : null;
}

/** Premium-only extras spread into each generate call. Non-premium callers get
 *  an empty object so stale session values can never reach the command. */
function premiumExtras(
  instructions: string,
  words: string | number
): { additionalInstructions?: string; targetWordCount?: number | null } {
  if (!isPremium.value) return {};
  return {
    additionalInstructions: instructions,
    targetWordCount: parseTargetWords(words),
  };
}

/** Switch-vs-regenerate dialog. When target report already has content, open
 *  this instead of regenerating blindly. User can view existing or regenerate.
 *  `pendingKind` records which report was clicked. */
const pendingKind = ref<'review' | 'gap' | null>(null);
const showSwitchDialog = ref(false);

/** Human-readable name for the pending report kind, used in the dialog. */
const pendingKindLabel = computed(() =>
  pendingKind.value === 'gap' ? 'Research Gap Report' : 'Literature Review'
);

const CITATION_STYLES: CitationStyle[] = ['APA', 'MLA', 'Chicago', 'IEEE', 'AMA'];

const includedCount = computed(() => articlesStore.byStatus.included);
const hasAims = computed(() => criteriaStore.aims.length > 0);
// Canonical LLM-configured gate (wraps `useLlmConfigStore().isConfigured`).
const hasLlmConfig = useLlmConfigured();

const canGenerate = computed(() => includedCount.value > 0 && hasAims.value && hasLlmConfig.value);

/** Any generation in flight. Cross-disables both generate buttons. */
const anyLoading = computed(() => summaryLoading.value || gapLoading.value);

/** Active error is mode-specific. */
const activeError = computed(() => (mode.value === 'review' ? summaryError.value : gapError.value));

/** Active output text is mode-specific. */
const activeText = computed(() => (mode.value === 'review' ? summaryText.value : gapText.value));

/** Active generated-at formatter is mode-specific. */
function activeFormatGeneratedAt(): string | null {
  return mode.value === 'review' ? formatSummaryGeneratedAt() : formatGapGeneratedAt();
}

const missingRequirements = computed<string[]>(() => {
  const missing: string[] = [];
  if (!hasLlmConfig.value) {
    missing.push('Configure an AI provider in Settings');
  }
  if (includedCount.value === 0) {
    missing.push('Screen articles and mark some as Included');
  }
  if (!hasAims.value) {
    missing.push('Define research aims in Criteria');
  }
  return missing;
});

/** Rendered HTML of active report. Watch-driven to repaint after live LLM completion. */
const renderedHtml = ref<string>('');

watch(
  activeText,
  (text) => {
    renderedHtml.value = text ? (marked.parse(text) as string) : '';
  },
  { immediate: true }
);

/** Entry point for the "Summarize Findings" button. If the target report
 *  already has content, open the switch-vs-regenerate dialog; otherwise
 *  generate directly. */
function handleGenerateSummary(): void {
  if (!canGenerate.value || anyLoading.value) return;
  if (summaryText.value) {
    pendingKind.value = 'review';
    showSwitchDialog.value = true;
  } else {
    void doGenerate('review');
  }
}

/** Entry point for the "Research Gap Report" button. Same dialog logic. */
function handleGenerateGap(): void {
  if (!canGenerate.value || anyLoading.value) return;
  if (gapText.value) {
    pendingKind.value = 'gap';
    showSwitchDialog.value = true;
  } else {
    void doGenerate('gap');
  }
}

/** Switch to the existing report (no LLM call). Closes the dialog. */
function viewExisting(): void {
  if (pendingKind.value === 'gap') {
    mode.value = 'gaps';
  } else {
    mode.value = 'review';
  }
  closeSwitchDialog();
}

/** Regenerate the pending report (overwrites the persisted version). Closes
 *  the dialog and kicks off the LLM call via `doGenerate`. */
function regenerateExisting(): void {
  const kind = pendingKind.value;
  closeSwitchDialog();
  if (kind) void doGenerate(kind);
}

/** Close the switch-vs-regenerate dialog without taking any action. */
function closeSwitchDialog(): void {
  showSwitchDialog.value = false;
  pendingKind.value = null;
}

/** Shared generation path. Calls the matching composable, then switches
 *  `mode` AFTER the await resolves so the output area stays on the current
 *  report until the new one is ready (avoids the "text switches immediately"
 *  flash of the old saved report). Each report forwards only its own guidance
 *  card's values (premium-gated via `premiumExtras`). */
async function doGenerate(kind: 'review' | 'gap'): Promise<void> {
  if (kind === 'review') {
    await generateSummary({
      style: citationStyle.value,
      ...premiumExtras(reviewInstructions.value, reviewTargetWords.value),
    });
    mode.value = 'review';
  } else {
    await generateGap({
      style: citationStyle.value,
      ...premiumExtras(gapInstructions.value, gapTargetWords.value),
    });
    mode.value = 'gaps';
  }
}

async function copyToClipboard(): Promise<void> {
  if (!activeText.value) return;
  await navigator.clipboard.writeText(activeText.value);
  copied.value = true;
  setTimeout(() => {
    copied.value = false;
  }, 2000);
}

/** Export filename switches with mode so the saved file matches the content. */
function exportFilename(): string {
  return mode.value === 'review' ? 'literature-review.md' : 'research-gaps.md';
}

async function exportMarkdown(): Promise<void> {
  if (!activeText.value) return;
  try {
    const filePath = await save({
      defaultPath: exportFilename(),
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (filePath) {
      await tauriCommand('write_text_to_file', { path: filePath, content: activeText.value });
    }
  } catch (e: unknown) {
    console.error('Failed to export markdown:', e);
  }
}

function exportPdf(): void {
  if (!activeText.value) return;

  const printTitle = mode.value === 'review' ? 'Literature Review' : 'Research Gaps';

  // Create a hidden iframe to avoid Tauri's window.open restriction
  const iframe = document.createElement('iframe');
  iframe.style.position = 'fixed';
  iframe.style.right = '0';
  iframe.style.bottom = '0';
  iframe.style.width = '0';
  iframe.style.height = '0';
  iframe.style.border = 'none';
  document.body.appendChild(iframe);

  const doc = iframe.contentDocument || iframe.contentWindow?.document;
  if (!doc) {
    document.body.removeChild(iframe);
    return;
  }

  doc.open();
  doc.write(`<!DOCTYPE html>
<html>
<head>
  <title>${printTitle}</title>
  <style>
    body {
      font-family: 'Georgia', 'Times New Roman', serif;
      max-width: 800px;
      margin: 40px auto;
      padding: 0 20px;
      line-height: 1.8;
      color: #1b1b24;
      font-size: 14px;
    }
    h1 { font-size: 20px; margin-bottom: 24px; border-bottom: 1px solid #ddd; padding-bottom: 8px; }
    h2 { font-size: 16px; margin-top: 28px; margin-bottom: 12px; }
    h3 { font-size: 14px; margin-top: 20px; margin-bottom: 8px; }
    p { margin-bottom: 12px; text-align: justify; }
    ul, ol { margin-bottom: 12px; padding-left: 24px; }
    li { margin-bottom: 4px; }
    blockquote { border-left: 3px solid #ccc; padding-left: 16px; color: #555; margin: 12px 0; }
    strong { font-weight: 600; }
    @media print {
      body { margin: 0; padding: 20px; }
    }
  </style>
</head>
<body>
  ${renderedHtml.value}
</body>
</html>`);
  doc.close();

  // Wait for content to render then print
  setTimeout(() => {
    iframe.contentWindow?.focus();
    iframe.contentWindow?.print();
    // Clean up after print dialog
    setTimeout(() => {
      document.body.removeChild(iframe);
    }, 1000);
  }, 500);
}

onMounted(async () => {
  // Force refresh articles to get accurate included count
  await Promise.all([
    articlesStore.fetchArticles(),
    criteriaStore.fetchIfNeeded(),
    // `useLlmConfigured()` already pre-warms the store on first read, so no
    // explicit `fetchIfNeeded()` is needed here.
  ]);
  // Restore previously saved outputs for both modes.
  await Promise.all([loadSavedSummary(), loadSavedGap()]);
});
</script>

<template>
  <div class="summary-view">
    <!-- Header -->
    <header class="summary-header">
      <div>
        <h1 class="page-title">AI Summary</h1>
        <p class="summary-header__tagline">
          Have AI create a summary based on included papers. (AI makes mistakes)
        </p>
      </div>
      <div class="summary-header__actions">
        <!-- Two side-by-side generate buttons. The left (Research Gap Report)
             is secondary style; the right (Summarize Findings) is primary.
             Clicking either switches the output area to that report and
             regenerates it. While either is running, both are disabled. -->
        <button
          class="btn btn--primary"
          :disabled="!canGenerate || anyLoading"
          @click="handleGenerateGap"
        >
          <template v-if="gapLoading">
            <span class="material-symbols-outlined btn__spinner">progress_activity</span>
            Generating...
          </template>
          <template v-else>
            <span class="material-symbols-outlined btn__icon">lightbulb</span>
            Research Gap Report
          </template>
        </button>
        <button
          class="btn btn--primary"
          :disabled="!canGenerate || anyLoading"
          @click="handleGenerateSummary"
        >
          <template v-if="summaryLoading">
            <span class="material-symbols-outlined btn__spinner">progress_activity</span>
            Generating...
          </template>
          <template v-else>
            <span class="material-symbols-outlined btn__icon">auto_awesome</span>
            Summarize Findings
          </template>
        </button>
      </div>
    </header>

    <!-- Requirements status -->
    <div v-if="!canGenerate" class="summary-requirements">
      <span class="material-symbols-outlined summary-requirements__icon">info</span>
      <div class="summary-requirements__content">
        <p class="summary-requirements__title">Before you can generate a report:</p>
        <ul class="summary-requirements__list">
          <li v-for="(req, idx) in missingRequirements" :key="idx">{{ req }}</li>
        </ul>
      </div>
    </div>

    <!-- Error -->
    <div v-if="activeError" class="summary-view__error">
      <span class="material-symbols-outlined">error</span>
      {{ activeError }}
    </div>

    <!-- Output toolbar + content (always visible when requirements met) -->
    <div v-if="canGenerate" class="summary-view__output">
      <!-- Premium per-report generation guidance. Two collapsible cards between
           the header and the Citation Style toolbar; each generate button uses
           only its own card's values. Hidden entirely when not premium. -->
      <template v-if="isPremium">
        <div class="summary-guidance">
          <button
            type="button"
            class="summary-guidance__toggle"
            :aria-expanded="showReviewGuidance"
            aria-controls="review-guidance-body"
            @click="showReviewGuidance = !showReviewGuidance"
          >
            <span class="material-symbols-outlined summary-guidance__chevron">
              {{ showReviewGuidance ? 'expand_less' : 'expand_more' }}
            </span>
            <span>Literature Review Instructions</span>
          </button>
          <div v-if="showReviewGuidance" id="review-guidance-body" class="summary-guidance__body">
            <div class="summary-guidance__field">
              <label for="review-instructions" class="summary-guidance__label">
                Additional instructions for the LLM
              </label>
              <textarea
                id="review-instructions"
                v-model="reviewInstructions"
                class="summary-guidance__textarea"
                rows="4"
                placeholder="e.g. Focus on policy outcomes and emphasize longitudinal designs"
              ></textarea>
            </div>
            <div class="summary-guidance__field summary-guidance__field--words">
              <label for="review-target-words" class="summary-guidance__label">
                Target length (words)
              </label>
              <input
                id="review-target-words"
                v-model="reviewTargetWords"
                class="summary-guidance__input"
                type="number"
                min="1"
                max="50000"
                step="1"
                inputmode="numeric"
                placeholder="No limit"
              />
            </div>
          </div>
        </div>
        <div class="summary-guidance">
          <button
            type="button"
            class="summary-guidance__toggle"
            :aria-expanded="showGapGuidance"
            aria-controls="gap-guidance-body"
            @click="showGapGuidance = !showGapGuidance"
          >
            <span class="material-symbols-outlined summary-guidance__chevron">
              {{ showGapGuidance ? 'expand_less' : 'expand_more' }}
            </span>
            <span>Research Gap Report Instructions</span>
          </button>
          <div v-if="showGapGuidance" id="gap-guidance-body" class="summary-guidance__body">
            <div class="summary-guidance__field">
              <label for="gap-instructions" class="summary-guidance__label">
                Additional instructions for the LLM
              </label>
              <textarea
                id="gap-instructions"
                v-model="gapInstructions"
                class="summary-guidance__textarea"
                rows="4"
                placeholder="e.g. Highlight methodological weaknesses and geographic blind spots"
              ></textarea>
            </div>
            <div class="summary-guidance__field summary-guidance__field--words">
              <label for="gap-target-words" class="summary-guidance__label">
                Target length (words)
              </label>
              <input
                id="gap-target-words"
                v-model="gapTargetWords"
                class="summary-guidance__input"
                type="number"
                min="1"
                max="50000"
                step="1"
                inputmode="numeric"
                placeholder="No limit"
              />
            </div>
          </div>
        </div>
      </template>
      <div class="summary-toolbar">
        <!-- Left: Citation style -->
        <div class="summary-toolbar__style">
          <label for="citation-style" class="summary-toolbar__label">Citation Style</label>
          <select id="citation-style" v-model="citationStyle" class="summary-toolbar__select">
            <option v-for="style in CITATION_STYLES" :key="style" :value="style">
              {{ style }}
            </option>
          </select>
        </div>
        <!-- Center: Meta info -->
        <div class="summary-toolbar__meta">
          {{ includedCount }} article{{ includedCount !== 1 ? 's' : '' }} &middot;
          {{ criteriaStore.aims.length }} aim{{ criteriaStore.aims.length !== 1 ? 's' : '' }}
          <template v-if="activeFormatGeneratedAt()">
            &middot; generated: {{ activeFormatGeneratedAt() }}
          </template>
        </div>
        <!-- Right: Export buttons -->
        <div class="summary-toolbar__exports">
          <button class="btn btn--secondary" :disabled="!activeText" @click="copyToClipboard">
            <span class="material-symbols-outlined btn__icon">content_copy</span>
            {{ copied ? 'Copied!' : 'Copy' }}
          </button>
          <button class="btn btn--secondary" :disabled="!activeText" @click="exportMarkdown">
            <span class="material-symbols-outlined btn__icon">description</span>
            Export Markdown
          </button>
          <button class="btn btn--secondary" :disabled="!activeText" @click="exportPdf">
            <span class="material-symbols-outlined btn__icon">picture_as_pdf</span>
            Export PDF
          </button>
        </div>
      </div>
      <!-- eslint-disable-next-line vue/no-v-html -- trusted LLM output rendered via marked -->
      <div v-if="activeText" class="summary-view__markdown markdown-body" v-html="renderedHtml" />
    </div>

    <!-- Switch-vs-regenerate dialog. Shown when the user clicks a generate
         button whose target report already has content. Offers two paths:
         view the existing report (no LLM call) or regenerate (overwrites). -->
    <div v-if="showSwitchDialog" class="dialog-overlay" @click.self="closeSwitchDialog">
      <div class="dialog">
        <h2 class="dialog__title">{{ pendingKindLabel }} already exists</h2>
        <p class="dialog__desc">
          A {{ pendingKindLabel.toLowerCase() }} has already been generated. Do you want to view the
          existing report or generate a new one?
        </p>
        <div class="dialog__actions">
          <button class="btn btn--secondary" @click="closeSwitchDialog">Cancel</button>
          <button class="btn btn--outline" @click="regenerateExisting">
            <span class="material-symbols-outlined btn__icon">refresh</span>
            Regenerate
          </button>
          <button class="btn btn--primary" @click="viewExisting">
            <span class="material-symbols-outlined btn__icon">visibility</span>
            View existing
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ── View Container (matches prisma-view) ── */
.summary-view {
  padding: var(--container-padding);
  display: flex;
  flex-direction: column;
  gap: var(--space-6);
  min-height: 100%;
}

/* ── Header (matches prisma-header) ── */
.summary-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-4);
}

.summary-header__tagline {
  font-size: var(--font-size-body);
  color: var(--color-on-surface-variant);
  margin-top: var(--space-1);
}

.summary-header__actions {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}

/* ── Requirements (matches prisma-error pattern) ── */
.summary-requirements {
  display: flex;
  gap: var(--space-3);
  padding: var(--space-4);
  background-color: var(--color-surface-container);
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-default);
}

.summary-requirements__icon {
  color: var(--color-on-surface-variant);
  font-size: 20px;
  flex-shrink: 0;
  margin-top: 2px;
}

.summary-requirements__content {
  flex: 1;
}

.summary-requirements__title {
  font-weight: var(--font-weight-semibold);
  font-size: var(--font-size-body);
  margin: 0 0 var(--space-2) 0;
  color: var(--color-on-surface);
}

.summary-requirements__list {
  margin: 0;
  padding-left: var(--space-4);
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  line-height: 1.8;
}

/* ── Toolbar ── */
.summary-toolbar {
  display: flex;
  align-items: flex-end;
  gap: var(--space-4);
  flex-wrap: wrap;
  flex-shrink: 0;
}

.summary-toolbar__style {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.summary-toolbar__label {
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
}

.summary-toolbar__select {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-body);
  font-family: var(--font-family);
  background-color: var(--color-surface-container-low);
  color: var(--color-on-surface);
  min-width: 140px;
  cursor: pointer;
}

.summary-toolbar__select:focus {
  outline: 2px solid var(--color-primary);
  outline-offset: -1px;
}

.summary-toolbar__meta {
  flex: 1;
  font-size: var(--font-size-caption);
  color: var(--color-on-surface-variant);
  white-space: nowrap;
  padding-bottom: var(--space-2);
}

.summary-toolbar__exports {
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}

/* ── Premium guidance cards (between header and toolbar) ── */
.summary-guidance {
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-default);
  background-color: var(--color-surface-container-low);
  flex-shrink: 0;
}

.summary-guidance__toggle {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: none;
  border-radius: var(--radius-default);
  background-color: transparent;
  font-size: var(--font-size-label);
  font-weight: var(--font-weight-semibold);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
  color: var(--color-on-surface-variant);
  font-family: var(--font-family);
  cursor: pointer;
}

.summary-guidance__toggle:hover {
  background-color: var(--color-surface-container);
}

.summary-guidance__chevron {
  font-size: 18px;
  flex-shrink: 0;
}

.summary-guidance__body {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  padding: 0 var(--space-3) var(--space-3);
}

.summary-guidance__field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  flex: 1;
  min-width: 240px;
}

.summary-guidance__field--words {
  flex: 0 0 150px;
  min-width: 150px;
}

.summary-guidance__label {
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  color: var(--color-on-surface-variant);
}

.summary-guidance__textarea {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-body);
  font-family: var(--font-family);
  background-color: var(--color-surface-container-low);
  color: var(--color-on-surface);
  resize: vertical;
  min-height: 90px;
}

.summary-guidance__input {
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--color-outline);
  border-radius: var(--radius-default);
  font-size: var(--font-size-body);
  font-family: var(--font-family);
  background-color: var(--color-surface-container-low);
  color: var(--color-on-surface);
  width: 100%;
}

.summary-guidance__textarea:focus,
.summary-guidance__input:focus {
  outline: 2px solid var(--color-primary);
  outline-offset: -1px;
}

/* ── Buttons (matches prisma buttons) ── */
.btn {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  border: 1px solid transparent;
  font-family: var(--font-family);
  transition: all 0.15s ease;
}

.btn__icon {
  font-size: 18px;
}

.btn--primary {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.btn--primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn--secondary {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}

.btn--secondary:hover:not(:disabled) {
  background-color: var(--color-surface-container-highest);
}

.btn--outline {
  background-color: transparent;
  color: var(--color-on-surface);
  border-color: var(--color-outline);
}

.btn--outline:hover:not(:disabled) {
  background-color: var(--color-surface-container);
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn__loading {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
}

.btn__spinner {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* ── Error (matches prisma-error) ── */
.summary-view__error {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
}

.summary-view__error .material-symbols-outlined {
  font-size: 18px;
  flex-shrink: 0;
}

/* ── Output ── */
.summary-view__output {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  flex: 1;
  min-height: 0;
}

/* ── Rendered markdown output ── */
.summary-view__markdown {
  padding: var(--space-5);
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-default);
  background-color: var(--color-surface-container-low);
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  line-height: 1.8;
  color: var(--color-on-surface);
}

/* Markdown body typography */
.markdown-body :deep(h1) {
  font-size: 1.5rem;
  font-weight: var(--font-weight-semibold);
  margin: 0 0 var(--space-4) 0;
  padding-bottom: var(--space-2);
  border-bottom: 1px solid var(--color-outline-variant);
}

.markdown-body :deep(h2) {
  font-size: 1.25rem;
  font-weight: var(--font-weight-semibold);
  margin: var(--space-5) 0 var(--space-3) 0;
}

.markdown-body :deep(h3) {
  font-size: 1.1rem;
  font-weight: var(--font-weight-semibold);
  margin: var(--space-4) 0 var(--space-2) 0;
}

.markdown-body :deep(p) {
  margin: 0 0 var(--space-3) 0;
  text-align: justify;
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin: 0 0 var(--space-3) 0;
  padding-left: var(--space-5);
}

.markdown-body :deep(li) {
  margin-bottom: var(--space-1);
}

.markdown-body :deep(blockquote) {
  border-left: 3px solid var(--color-primary);
  padding-left: var(--space-3);
  margin: var(--space-3) 0;
  color: var(--color-on-surface-variant);
  font-style: italic;
}

.markdown-body :deep(strong) {
  font-weight: var(--font-weight-semibold);
}

.markdown-body :deep(em) {
  font-style: italic;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--color-outline-variant);
  margin: var(--space-4) 0;
}

.markdown-body :deep(code) {
  background-color: var(--color-surface-container-high);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 0.9em;
}

.markdown-body :deep(pre) {
  background-color: var(--color-surface-container-high);
  padding: var(--space-3);
  border-radius: var(--radius-default);
  overflow-x: auto;
  margin: var(--space-3) 0;
}

.markdown-body :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: var(--space-3) 0;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid var(--color-outline-variant);
  padding: var(--space-2) var(--space-3);
  text-align: left;
}

.markdown-body :deep(th) {
  background-color: var(--color-surface-container);
  font-weight: var(--font-weight-semibold);
}

/* ── Responsive ── */
@media (max-width: 767px) {
  .summary-view {
    padding: var(--container-padding-sm);
    gap: var(--space-4);
  }

  .summary-header {
    flex-direction: column;
    align-items: flex-start;
  }

  .summary-header__actions {
    flex-wrap: wrap;
  }

  .summary-toolbar {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .summary-toolbar__exports {
    flex-wrap: wrap;
  }
}
</style>
