<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { useImport } from '@/composables/use-import';
import { useZotero } from '@/composables/use-zotero';
import ImportDropZone from '@/components/import-drop-zone.vue';
import ImportStepper from '@/components/import-stepper.vue';
import ImportPreview from '@/components/import-preview.vue';
import ZoteroCollectionPicker from '@/components/zotero-collection-picker.vue';

const router = useRouter();
const zotero = useZotero();

const {
  step,
  fileName,
  preview,
  importResult,
  loading,
  error,
  canImport,
  hasZeroValid,
  removedIndices,
  skipDuplicates,
  visibleCount,
  importableCount,
  dedupSummary,
  importSource,
  zoteroPreviewMeta,
  zoteroAttachmentSummary,
  loadFile,
  loadFilePath,
  parseFile,
  confirmImport,
  removeArticle,
  startZoteroImport,
  applyZoteroPreview,
  backToZoteroPicker,
  reset,
} = useImport();

const zoteroConnecting = ref(false);

/** Drop-zone "Import from Zotero": probe the connection, then enter the step. */
async function onZoteroSelected(): Promise<void> {
  zoteroConnecting.value = true;
  error.value = null;
  try {
    const ok = await zotero.checkConnection();
    if (ok) {
      startZoteroImport();
    }
    // Non-ok states render via zotero.connectionMessage below.
  } finally {
    zoteroConnecting.value = false;
  }
}

const hasDuplicates = () =>
  dedupSummary.value &&
  (dedupSummary.value.autoMergedCount > 0 || dedupSummary.value.needsReviewCount > 0);
</script>

<template>
  <div class="import-view">
    <div class="import-view__header">
      <h1 class="page-title">Import Articles</h1>
      <ImportStepper :current-step="step" />
    </div>

    <div v-if="error" class="import-view__error">
      {{ error }}
    </div>

    <div class="import-view__body">
      <!-- Step 1: Upload -->
      <section v-if="step === 'upload'">
        <ImportDropZone
          @file-selected="loadFile"
          @file-dropped="loadFilePath"
          @zotero-selected="onZoteroSelected"
        />
        <div v-if="zoteroConnecting" class="import-view__warning">
          <span class="spinner" /> Checking Zotero...
        </div>
        <div
          v-else-if="zotero.connectionState.value && zotero.connectionState.value !== 'ok'"
          class="import-view__warning"
        >
          {{ zotero.connectionMessage.value }}
          <button class="btn btn--outline" @click="onZoteroSelected">Retry</button>
        </div>
      </section>

      <!-- Step 2: Zotero collection picker -->
      <section v-if="step === 'zotero'">
        <ZoteroCollectionPicker @collection-selected="applyZoteroPreview" @back="reset" />
      </section>

      <!-- Step 2: Parse -->
      <section v-if="step === 'parse'">
        <p class="import-view__file-name">Selected: {{ fileName }}</p>
        <div class="import-view__actions">
          <button class="btn btn--outline" @click="reset">Cancel</button>
          <button class="btn btn--primary" :disabled="loading" @click="parseFile">
            {{ loading ? 'Parsing...' : 'Parse File' }}
          </button>
        </div>
      </section>

      <!-- Step 3: Review & Import -->
      <section v-if="step === 'import' && preview">
        <div v-if="hasZeroValid" class="import-view__warning">
          0 importable items in this collection. Go back and pick a different collection.
        </div>
        <div v-else-if="preview.errorCount > 0" class="import-view__warning">
          {{ preview.errorCount }} of {{ preview.totalRecords }} records have validation issues and
          will be skipped. Only {{ visibleCount }} valid articles can be imported.
        </div>

        <div v-if="zoteroPreviewMeta" class="import-view__zotero-meta">
          {{ zoteroPreviewMeta.totalItems }} items in "{{ zoteroPreviewMeta.collectionName }}" -
          {{ zoteroPreviewMeta.attachmentCount }} with full-text attachments,
          {{ zoteroPreviewMeta.noteCount }} with notes, {{ zoteroPreviewMeta.tagCount }} tags.
        </div>

        <div class="import-view__summary">
          <div class="import-view__stats-group">
            <div class="import-view__stat">
              <span class="import-view__stat-value">{{ preview.totalRecords }}</span>
              <span class="import-view__stat-label">Total Records</span>
            </div>
            <div class="import-view__stat">
              <span class="import-view__stat-value">{{ visibleCount }}</span>
              <span class="import-view__stat-label">Valid</span>
            </div>
            <div class="import-view__stat import-view__stat--error">
              <span class="import-view__stat-value">{{ preview.errorCount }}</span>
              <span class="import-view__stat-label">Skipped</span>
            </div>
            <div
              v-if="preview.duplicateCount > 0"
              class="import-view__stat import-view__stat--warn"
            >
              <span class="import-view__stat-value">{{ preview.duplicateCount }}</span>
              <span class="import-view__stat-label">Duplicates</span>
              <label class="import-view__skip" data-test="skip-duplicates">
                <input v-model="skipDuplicates" type="checkbox" :disabled="loading" />
                Skip
              </label>
            </div>
          </div>
          <div class="import-view__actions import-view__actions--inline">
            <button
              v-if="importSource === 'zotero'"
              class="btn btn--outline"
              :disabled="loading"
              @click="backToZoteroPicker"
            >
              Back to Collections
            </button>
            <button v-else class="btn btn--outline" @click="reset">Cancel</button>
            <button
              class="btn btn--primary"
              :disabled="!canImport || loading || importableCount === 0"
              @click="confirmImport"
            >
              {{ loading ? 'Importing...' : `Import ${importableCount} Articles` }}
            </button>
          </div>
        </div>

        <ImportPreview
          :articles="preview.previewArticles"
          :error-count="preview.errorCount"
          :error-groups="preview.errorGroups"
          :removed-indices="removedIndices"
          :total-valid-count="visibleCount"
          @remove="removeArticle"
        />
      </section>

      <!-- Step 4: Complete -->
      <section v-if="step === 'complete' && importResult">
        <div class="import-view__success">
          <h2>Import Complete</h2>
          <p>{{ importResult.importedCount }} articles imported successfully.</p>
          <p v-if="importResult.skippedCount > 0" class="import-view__skipped">
            {{ importResult.skippedCount }} record{{ importResult.skippedCount !== 1 ? 's' : '' }}
            skipped due to validation issues.
          </p>
          <p v-if="importResult.skippedByUser > 0" class="import-view__skipped">
            {{ importResult.skippedByUser }} record{{ importResult.skippedByUser !== 1 ? 's' : '' }}
            excluded by user.
          </p>
          <p v-if="importResult.skippedDuplicates > 0" class="import-view__skipped">
            {{ importResult.skippedDuplicates }} duplicate{{
              importResult.skippedDuplicates !== 1 ? 's' : ''
            }}
            skipped (already in library).
          </p>
          <p class="import-view__capacity">
            Remaining capacity: {{ importResult.remainingCapacity }} articles
          </p>
          <div v-if="zoteroAttachmentSummary" class="import-view__zotero-summary">
            <p>
              {{ zoteroAttachmentSummary.attachedCount }} full-text file{{
                zoteroAttachmentSummary.attachedCount !== 1 ? 's' : ''
              }}
              attached from Zotero.
            </p>
            <p v-if="zoteroAttachmentSummary.failedCount > 0" class="import-view__skipped">
              {{ zoteroAttachmentSummary.failedCount }} attachment{{
                zoteroAttachmentSummary.failedCount !== 1 ? 's' : ''
              }}
              failed (see the articles' audit timelines).
            </p>
            <p v-if="zoteroAttachmentSummary.skippedCount > 0" class="import-view__skipped">
              {{ zoteroAttachmentSummary.skippedCount }} non-pdf/txt attachment{{
                zoteroAttachmentSummary.skippedCount !== 1 ? 's' : ''
              }}
              skipped.
            </p>
            <p
              v-if="zoteroPreviewMeta && zoteroPreviewMeta.tagCount > 0"
              class="import-view__skipped"
            >
              {{ zoteroPreviewMeta.tagCount }} Zotero tag{{
                zoteroPreviewMeta.tagCount !== 1 ? 's' : ''
              }}
              imported as Bango tags.
            </p>
          </div>
        </div>

        <!-- Dedup summary -->
        <div v-if="hasDuplicates()" class="import-view__dedup">
          <h3>Duplicate Check</h3>
          <p v-if="dedupSummary!.autoMergedCount > 0">
            🔍 {{ dedupSummary!.autoMergedCount }} high-confidence duplicate{{
              dedupSummary!.autoMergedCount !== 1 ? 's' : ''
            }}
            detected.
          </p>
          <p v-if="dedupSummary!.needsReviewCount > 0">
            ⚠️ {{ dedupSummary!.needsReviewCount }} potential duplicate{{
              dedupSummary!.needsReviewCount !== 1 ? 's' : ''
            }}
            need manual review.
          </p>
          <button class="btn btn--primary" @click="router.push('/dedup')">Review Duplicates</button>
        </div>

        <div class="import-view__actions">
          <button class="btn btn--secondary" @click="reset">Import Another File</button>
          <button class="btn btn--primary" @click="router.push('/')">Go to Dashboard</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.import-view {
  padding: var(--container-padding);
  max-width: 900px;
  margin: 0 auto;
}

@media (max-width: 767px) {
  .import-view {
    padding: var(--container-padding-sm);
  }

  .import-view__summary {
    flex-wrap: wrap;
  }
}

.import-view__header {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-bottom: var(--space-6);
}

.import-view__error {
  padding: var(--space-3);
  background-color: var(--color-error-container);
  color: var(--color-error);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
}

.import-view__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.import-view__file-name {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.import-view__summary {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
  margin-bottom: var(--space-4);
}

.import-view__stats-group {
  display: flex;
  gap: var(--space-4);
}

.import-view__stat {
  display: flex;
  flex-direction: column;
  padding: var(--space-3) var(--space-4);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  min-width: 100px;
}

.import-view__stat-value {
  font-size: var(--font-size-h1);
  font-weight: var(--font-weight-semibold);
}

.import-view__stat-label {
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  letter-spacing: var(--letter-spacing-label);
  text-transform: uppercase;
}

.import-view__stat--error .import-view__stat-value {
  color: var(--color-error);
}

.import-view__stat--warn .import-view__stat-value {
  color: var(--color-warning, #b26a00);
}

.import-view__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
  margin-top: var(--space-4);
}

.import-view__actions--inline {
  margin-top: 0;
  align-self: flex-end;
}

.import-view__success {
  padding: var(--space-6);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-default);
  text-align: center;
}

.import-view__success h2 {
  margin-bottom: var(--space-2);
}

.import-view__warning {
  padding: var(--space-3);
  background-color: var(--color-warning-container, #fef3cd);
  color: var(--color-on-warning-container, #664d03);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-4);
}

.import-view__skip {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  font-size: var(--font-size-label);
  color: var(--color-on-surface-variant);
  cursor: pointer;
  margin-top: var(--space-1);
}

.import-view__skipped {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  margin-top: var(--space-1);
}

.import-view__capacity {
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  margin-top: var(--space-2);
}

.import-view__zotero-meta {
  padding: var(--space-2) var(--space-3);
  background-color: var(--color-surface-container);
  border-radius: var(--radius-default);
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-3);
}

.import-view__zotero-summary {
  margin-top: var(--space-2);
  padding-top: var(--space-3);
  border-top: 1px solid var(--color-outline-variant);
  color: var(--color-on-surface-variant);
  font-size: var(--font-size-caption);
}

.import-view__zotero-summary p {
  margin-bottom: var(--space-1);
}

.import-view__dedup {
  margin-top: var(--space-4);
  padding: var(--space-4);
  background-color: var(--color-surface-container-low);
  border-radius: var(--radius-default);
  border-left: 3px solid var(--color-primary);
}

.import-view__dedup h3 {
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-semibold);
  margin-bottom: var(--space-2);
}

.import-view__dedup p {
  font-size: var(--font-size-caption);
  margin-bottom: var(--space-2);
}

.import-view__dedup .btn {
  margin-top: var(--space-2);
}

.btn {
  padding: var(--space-2) var(--space-4);
  border-radius: var(--radius-default);
  font-size: var(--font-size-caption);
  font-weight: var(--font-weight-semibold);
  cursor: pointer;
  transition: opacity 0.15s;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn--primary {
  background-color: var(--color-primary);
  color: var(--color-on-primary);
}

.btn--secondary {
  background-color: var(--color-surface-container-high);
  color: var(--color-on-surface);
}
.btn--outline {
  background: transparent;
  color: var(--color-on-surface-variant, #464555);
  border: 1px solid var(--color-outline, #777587);
}
</style>
