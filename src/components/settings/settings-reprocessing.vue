<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

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

// Load the count on mount so the chunk-rebuild section shows only when relevant.
loadFullTextCount();
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

    <!-- Batch Import (placeholder) -->
    <div class="batch-import">
      <div class="batch-import__row">
        <span class="batch-import__title">Batch Import</span>
        <span class="batch-import__badge" title="This feature is under development.">
          Coming soon
        </span>
      </div>
      <p class="batch-import__desc">
        Import multiple RIS or BibTeX files at once and optionally auto-attach full-text PDFs from a
        watched directory.
      </p>
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

/* ── Batch Import placeholder ── */
.batch-import {
  margin-top: 1.25rem;
  padding-top: 1.25rem;
  border-top: 1px solid var(--color-surface-variant, #e4e1ee);
}

.batch-import__row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.batch-import__title {
  font-size: var(--font-size-body, 14px);
  font-weight: var(--font-weight-semibold, 600);
  color: var(--color-on-surface, #1b1b24);
}

.batch-import__badge {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  background-color: var(--color-surface-variant, #e4e1ee);
  color: var(--color-on-surface-variant, #464555);
}

.batch-import__desc {
  font-size: var(--font-size-caption, 12px);
  color: var(--color-on-surface-variant, #464555);
  line-height: 1.4;
  margin-top: 0.5rem;
}
</style>
