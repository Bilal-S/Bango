<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ── Text chunks for screening (moved from the old Full-Text Storage card) ──
interface RebuildChunksResult {
  success: boolean;
  chunked: number;
  failed: number;
  skipped: number;
  message: string;
}
const fullTextArticleCount = ref(0);
const rebuildLoading = ref(false);
const rebuildResult = ref<RebuildChunksResult | null>(null);
const rebuildError = ref<string | null>(null);

async function loadFullTextCount(): Promise<void> {
  try {
    fullTextArticleCount.value = await invoke<number>('count_articles_with_full_text');
  } catch {
    // Non-fatal: the rebuild section is hidden when the count is unknown (0).
    fullTextArticleCount.value = 0;
  }
}

async function rebuildChunks(): Promise<void> {
  rebuildLoading.value = true;
  rebuildError.value = null;
  rebuildResult.value = null;
  try {
    rebuildResult.value = await invoke<RebuildChunksResult>('rebuild_article_chunks');
  } catch (e: unknown) {
    rebuildError.value = e instanceof Error ? e.message : String(e);
  } finally {
    rebuildLoading.value = false;
  }
}

// ── Batch Import ──
interface BatchImportPhaseResult {
  total: number;
  processed: number;
  succeeded: number;
  failed: number;
  errors: string[];
}

interface BatchImportProgress {
  phase: number;
  phaseName: string;
  completed: number;
  total: number;
  overallPercent: number;
  message: string;
  isRunning: boolean;
  isCancelled: boolean;
  fullText: BatchImportPhaseResult | null;
  citations: BatchImportPhaseResult | null;
  summaries: BatchImportPhaseResult | null;
}

const showBatchDialog = ref(false);
const batchProgress = ref<BatchImportProgress | null>(null);
const batchError = ref<string | null>(null);
let batchUnlisten: UnlistenFn | null = null;

/** Read the auto-summarize + section-summaries localStorage flags. */
function readAutoSummarize(): boolean {
  return localStorage.getItem('bango-full-text-summaries') === 'true';
}
function readSectionSummaries(): boolean {
  return localStorage.getItem('bango-section-summaries') === 'true';
}

/** Start the batch import pipeline after the user confirms in the dialog. */
async function startBatchImport(): Promise<void> {
  showBatchDialog.value = false;
  batchError.value = null;
  batchProgress.value = null;
  try {
    await invoke<BatchImportProgress>('start_batch_import', {
      autoSummarize: readAutoSummarize(),
      includeSectionSummaries: readSectionSummaries(),
    });
  } catch (e: unknown) {
    batchError.value = e instanceof Error ? e.message : String(e);
  }
}

/** Cancel a running batch import. */
async function cancelBatchImport(): Promise<void> {
  try {
    await invoke('cancel_batch_import');
  } catch (e: unknown) {
    batchError.value = e instanceof Error ? e.message : String(e);
  }
}

/** Fetch the latest progress snapshot (for restoring state on mount). */
async function refreshBatchProgress(): Promise<void> {
  try {
    batchProgress.value = await invoke<BatchImportProgress>('get_batch_import_progress');
  } catch {
    // Non-fatal.
  }
}

onMounted(async () => {
  await loadFullTextCount();
  await refreshBatchProgress();
  // Listen for progress events so the bar updates live.
  batchUnlisten = await listen<BatchImportProgress>('batch-import:progress', (event) => {
    batchProgress.value = event.payload;
  });
});

onUnmounted(() => {
  if (batchUnlisten) {
    batchUnlisten();
  }
});
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">cached</span>
      Re-processing
    </h2>
    <p class="settings-card__desc">
      Rebuild derived data from your existing articles and attachments.
    </p>

    <!-- Text chunks for screening -->
    <div v-if="fullTextArticleCount > 0" class="rebuild-chunks">
      <div class="rebuild-chunks__row">
        <span class="rebuild-chunks__title">Text chunks for screening</span>
        <button class="btn btn--secondary" :disabled="rebuildLoading" @click="rebuildChunks">
          <span class="material-symbols-outlined btn__icon">cached</span>
          Rebuild text chunks
        </button>
      </div>
      <p class="rebuild-chunks__desc">
        Enhanced / Two-stage screening retrieves criteria-matched chunks from attached full text.
        Rebuild if chunks are missing (e.g. PDFs attached before this feature shipped) or after a
        chunking-algorithm update.
      </p>
      <p v-if="rebuildLoading" class="rebuild-chunks__status">Rebuilding chunks...</p>
      <p v-if="rebuildResult" class="rebuild-chunks__status rebuild-chunks__status--ok">
        {{ rebuildResult.message }}
      </p>
      <p v-if="rebuildError" class="rebuild-chunks__status rebuild-chunks__status--err">
        {{ rebuildError }}
      </p>
    </div>

    <!-- Batch Import -->
    <div class="batch-import">
      <div class="batch-import__row">
        <span class="batch-import__title">Batch Import</span>
        <button
          class="btn btn--secondary"
          :disabled="batchProgress?.isRunning === true"
          @click="showBatchDialog = true"
        >
          <span class="material-symbols-outlined btn__icon">upload</span>
          Import full text files
        </button>
      </div>
      <p class="batch-import__desc">
        Scan the Bango Documents directory for full-text PDFs, Citation Chaser RIS files, and
        optionally generate AI summaries. Files are matched to articles by DOI.
      </p>

      <!-- Progress bar (shown while running or after completion) -->
      <div v-if="batchProgress" class="batch-progress">
        <div class="batch-progress__header">
          <span class="batch-progress__phase">{{ batchProgress.phaseName }}</span>
          <span class="batch-progress__percent">{{ batchProgress.overallPercent }}%</span>
        </div>
        <div class="batch-progress__track">
          <div
            class="batch-progress__fill"
            :style="{ width: `${batchProgress.overallPercent}%` }"
            :class="{ 'batch-progress__fill--cancelled': batchProgress.isCancelled }"
          />
        </div>
        <p class="batch-progress__message">{{ batchProgress.message }}</p>
        <p v-if="batchProgress.total > 0" class="batch-progress__detail">
          {{ batchProgress.completed }} / {{ batchProgress.total }} items in current phase
        </p>

        <!-- Phase summary (shown after each phase completes) -->
        <div v-if="batchProgress.fullText" class="batch-progress__summary">
          Phase 1 (Full Text): {{ batchProgress.fullText.succeeded }} attached,
          {{ batchProgress.fullText.failed }} failed
        </div>
        <div v-if="batchProgress.citations" class="batch-progress__summary">
          Phase 2 (Citations): {{ batchProgress.citations.succeeded }} imported,
          {{ batchProgress.citations.failed }} failed
        </div>
        <div v-if="batchProgress.summaries" class="batch-progress__summary">
          Phase 3 (AI Summaries): {{ batchProgress.summaries.succeeded }} summarized,
          {{ batchProgress.summaries.failed }} failed
        </div>

        <!-- Cancel button -->
        <button
          v-if="batchProgress.isRunning && !batchProgress.isCancelled"
          class="btn btn--danger batch-progress__cancel"
          @click="cancelBatchImport"
        >
          <span class="material-symbols-outlined btn__icon">stop</span>
          Cancel
        </button>
      </div>

      <p v-if="batchError" class="rebuild-chunks__status rebuild-chunks__status--err">
        {{ batchError }}
      </p>
    </div>

    <!-- Batch Import Dialog -->
    <div v-if="showBatchDialog" class="dialog-overlay" @click.self="showBatchDialog = false">
      <div class="dialog">
        <h2>Batch Import</h2>
        <p class="dialog__desc">
          This will scan your Bango Documents directory and import files that match your articles by
          DOI. The pipeline runs in three phases:
        </p>
        <ol class="dialog__list">
          <li>
            <strong>Full Text</strong> - attaches PDF/TXT files from <code>fulltext/</code> to
            matching articles.
          </li>
          <li>
            <strong>Citations</strong> - imports Citation Chaser RIS/BibTeX files from
            <code>ris/</code>.
          </li>
          <li>
            <strong>AI Summaries</strong>
            <span v-if="readAutoSummarize()"
              >- generates summaries for newly-attached articles.</span
            >
            <span v-else class="dialog__note"
              >- skipped (auto-summarize is disabled in Settings).</span
            >
          </li>
        </ol>

        <div class="dialog__info-box">
          <span class="material-symbols-outlined">info</span>
          <div>
            <p><strong>File naming convention:</strong></p>
            <ul class="dialog__naming">
              <li>
                <code>fulltext/{doi}.pdf</code> - full text (DOI with <code>/</code> replaced by
                <code>_</code>)
              </li>
              <li><code>ris/{doi}_references.ris</code> - backward references</li>
              <li><code>ris/{doi}_citations.ris</code> - forward citations</li>
            </ul>
            <p class="dialog__example">
              Example: DOI <code>10.1016/j.jand.2021.06.013</code> becomes
              <code>10.1016_j.jand.2021.06.013.pdf</code>
            </p>
          </div>
        </div>

        <p class="dialog__hint">
          Articles that already have full text, references, or summaries are skipped.
        </p>

        <div class="dialog__actions">
          <button class="btn btn--outline" @click="showBatchDialog = false">Cancel</button>
          <button class="btn btn--primary" @click="startBatchImport">
            <span class="material-symbols-outlined btn__icon">play_arrow</span>
            Start
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.rebuild-chunks {
  padding-bottom: 1rem;
}

.rebuild-chunks__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.rebuild-chunks__title {
  font-size: var(--font-size-body, 14px);
  font-weight: var(--font-weight-semibold, 600);
  color: var(--color-on-surface, #1b1b24);
}

.rebuild-chunks__desc {
  font-size: var(--font-size-caption, 12px);
  color: var(--color-on-surface-variant, #464555);
  line-height: 1.4;
  margin-top: 0.5rem;
}

.rebuild-chunks__status {
  font-size: var(--font-size-caption, 12px);
  margin-top: 0.5rem;
}

.rebuild-chunks__status--ok {
  color: #15803d;
}

.rebuild-chunks__status--err {
  color: #991b1b;
}

/* ── Batch Import ── */
.batch-import {
  margin-top: 1.25rem;
  padding-top: 1.25rem;
  border-top: 1px solid var(--color-surface-variant, #e4e1ee);
}

.batch-import__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.75rem;
}

.batch-import__title {
  font-size: var(--font-size-body, 14px);
  font-weight: var(--font-weight-semibold, 600);
  color: var(--color-on-surface, #1b1b24);
}

.batch-import__desc {
  font-size: var(--font-size-caption, 12px);
  color: var(--color-on-surface-variant, #464555);
  line-height: 1.4;
  margin-top: 0.5rem;
}

/* ── Progress bar ── */
.batch-progress {
  margin-top: 0.75rem;
  padding: 0.75rem 1rem;
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 0.5rem;
}

.batch-progress__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.375rem;
}

.batch-progress__phase {
  font-size: var(--font-size-body, 14px);
  font-weight: var(--font-weight-semibold, 600);
  color: var(--color-primary, #3525cd);
}

.batch-progress__percent {
  font-size: var(--font-size-body, 14px);
  font-weight: 700;
  color: var(--color-on-surface, #1b1b24);
}

.batch-progress__track {
  height: 12px;
  background-color: var(--color-surface-container-high, #e4e1ee);
  border-radius: var(--radius-pill, 9999px);
  overflow: hidden;
}

.batch-progress__fill {
  height: 100%;
  background-color: var(--color-primary, #3525cd);
  border-radius: var(--radius-pill, 9999px);
  transition: width 0.5s ease;
}

.batch-progress__fill--cancelled {
  background-color: #991b1b;
}

.batch-progress__message {
  font-size: var(--font-size-caption, 12px);
  color: var(--color-on-surface-variant, #464555);
  margin-top: 0.375rem;
}

.batch-progress__detail {
  font-size: 11px;
  color: var(--color-on-surface-variant, #464555);
  margin-top: 0.125rem;
}

.batch-progress__summary {
  font-size: 11px;
  color: var(--color-on-surface-variant, #464555);
  margin-top: 0.125rem;
}

.batch-progress__cancel {
  margin-top: 0.5rem;
}

/* ── Dialog ── */
.dialog__list {
  margin: 0.75rem 0;
  padding-left: 1.25rem;
  font-size: var(--font-size-body, 14px);
  line-height: 1.6;
  color: var(--color-on-surface, #1b1b24);
}

.dialog__list li {
  margin-bottom: 0.25rem;
}

.dialog__note {
  color: var(--color-on-surface-variant, #464555);
  font-style: italic;
}

.dialog__naming {
  margin: 0.375rem 0 0.5rem;
  padding-left: 1.25rem;
  font-size: var(--font-size-caption, 12px);
  line-height: 1.5;
  color: var(--color-on-surface-variant, #464555);
}

.dialog__naming code,
.dialog__example code {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 11px;
  background-color: var(--color-surface-container-low, #f5f2ff);
  padding: 0.0625rem 0.375rem;
  border-radius: 0.25rem;
}

.dialog__example {
  font-size: var(--font-size-caption, 12px);
  color: var(--color-on-surface-variant, #464555);
  margin-top: 0.375rem;
}

.dialog__hint {
  font-size: var(--font-size-caption, 12px);
  color: var(--color-on-surface-variant, #464555);
  margin-top: 0.75rem;
  line-height: 1.4;
}
</style>
