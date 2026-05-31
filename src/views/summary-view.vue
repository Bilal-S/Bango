<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { marked } from 'marked';
import { useSummary, type CitationStyle } from '@/composables/use-summary';
import { useArticlesStore } from '@/stores/articles';
import { useCriteriaStore } from '@/stores/criteria';
import { useLlmConfigStore } from '@/stores/llm-config';
import { save } from '@tauri-apps/plugin-dialog';
import { tauriCommand } from '@/composables/use-tauri-command';

const { summaryText, loading, error, citationStyle, loadSaved, generate, formatGeneratedAt } =
  useSummary();
const articlesStore = useArticlesStore();
const criteriaStore = useCriteriaStore();
const llmConfigStore = useLlmConfigStore();

const copied = ref(false);

const CITATION_STYLES: CitationStyle[] = ['APA', 'MLA', 'Chicago', 'IEEE', 'AMA'];

const includedCount = computed(() => articlesStore.byStatus.included);
const hasAims = computed(() => criteriaStore.aims.length > 0);
const hasLlmConfig = computed(() => llmConfigStore.config.apiKeyEncrypted !== null);

const canGenerate = computed(() => includedCount.value > 0 && hasAims.value && hasLlmConfig.value);

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

/** Render markdown summary as HTML */
const renderedHtml = computed(() => {
  if (!summaryText.value) return '';
  return marked.parse(summaryText.value) as string;
});

async function handleGenerate(): Promise<void> {
  if (!canGenerate.value || loading.value) return;
  await generate(citationStyle.value);
}

async function copyToClipboard(): Promise<void> {
  if (!summaryText.value) return;
  await navigator.clipboard.writeText(summaryText.value);
  copied.value = true;
  setTimeout(() => {
    copied.value = false;
  }, 2000);
}

async function exportMarkdown(): Promise<void> {
  if (!summaryText.value) return;
  try {
    const filePath = await save({
      defaultPath: 'literature-review.md',
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    });
    if (filePath) {
      await tauriCommand('write_text_to_file', { path: filePath, content: summaryText.value });
    }
  } catch (e: unknown) {
    console.error('Failed to export markdown:', e);
  }
}

function exportPdf(): void {
  if (!summaryText.value) return;

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
  <title>Literature Review</title>
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
    llmConfigStore.fetchIfNeeded(),
  ]);
  // Restore previously saved summary
  await loadSaved();
});
</script>

<template>
  <div class="summary-view">
    <!-- Header -->
    <header class="summary-header">
      <div>
        <h1 class="page-title">AI Summary</h1>
        <p class="summary-header__tagline">Have AI create a summary based on included papers.</p>
      </div>
      <div class="summary-header__actions">
        <button
          class="btn btn--primary"
          :disabled="!canGenerate || loading"
          @click="handleGenerate"
        >
          <span v-if="loading" class="btn__loading">
            <span class="material-symbols-outlined btn__spinner">progress_activity</span>
            Generating...
          </span>
          <span v-else>
            <span class="material-symbols-outlined btn__icon">auto_awesome</span>
            Summarize Findings
          </span>
        </button>
      </div>
    </header>

    <!-- Requirements status -->
    <div v-if="!canGenerate" class="summary-requirements">
      <span class="material-symbols-outlined summary-requirements__icon">info</span>
      <div class="summary-requirements__content">
        <p class="summary-requirements__title">Before you can generate a summary:</p>
        <ul class="summary-requirements__list">
          <li v-for="(req, idx) in missingRequirements" :key="idx">{{ req }}</li>
        </ul>
      </div>
    </div>

    <!-- Error -->
    <div v-if="error" class="summary-view__error">
      <span class="material-symbols-outlined">error</span>
      {{ error }}
    </div>

    <!-- Output toolbar + content (always visible when requirements met) -->
    <div v-if="canGenerate" class="summary-view__output">
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
          <template v-if="formatGeneratedAt()">
            &middot; generated: {{ formatGeneratedAt() }}
          </template>
        </div>
        <!-- Right: Export buttons -->
        <div class="summary-toolbar__exports">
          <button class="btn btn--secondary" :disabled="!summaryText" @click="copyToClipboard">
            <span class="material-symbols-outlined btn__icon">content_copy</span>
            {{ copied ? 'Copied!' : 'Copy' }}
          </button>
          <button class="btn btn--secondary" :disabled="!summaryText" @click="exportMarkdown">
            <span class="material-symbols-outlined btn__icon">description</span>
            Export Markdown
          </button>
          <button class="btn btn--secondary" :disabled="!summaryText" @click="exportPdf">
            <span class="material-symbols-outlined btn__icon">picture_as_pdf</span>
            Export PDF
          </button>
        </div>
      </div>
      <!-- eslint-disable-next-line vue/no-v-html -- trusted LLM output rendered via marked -->
      <div v-if="summaryText" class="summary-view__markdown markdown-body" v-html="renderedHtml" />
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
}

/* ── Rendered markdown output ── */
.summary-view__markdown {
  padding: var(--space-5);
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-default);
  background-color: var(--color-surface-container-low);
  max-height: 70vh;
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
