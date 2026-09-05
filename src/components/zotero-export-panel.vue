<script setup lang="ts">
import { computed, onMounted, watch } from 'vue';
import { useZoteroExport } from '@/composables/use-zotero-export';

const props = defineProps<{
  /** Export scope: the human-readable label ("Working tab", "Included articles", ...). */
  scopeLabel: string;
  /** Backend export scope (status + screening-errors-only), mirroring the RIS export. */
  status: string;
  screeningErrorsOnly: boolean;
}>();

const emit = defineEmits<{ back: [] }>();

const zotero = useZoteroExport();

onMounted(() => {
  void zotero.openPanel();
});

// Load the DOI-diff preview whenever a collection is selected while connected
// (covers both the default selection after openPanel and manual picks). The
// multi-source watch form compares each source independently.
watch([() => zotero.connectionState.value, () => zotero.selectedKey.value], ([state, key]) => {
  if (state === 'ok' && key) {
    void zotero.loadPreview(props.status, props.screeningErrorsOnly);
  }
});

/** Root-first tree ordering with children directly after their parents. */
const tree = computed(() => {
  const byParent = new Map<string | null, typeof zotero.collections.value>();
  for (const collection of zotero.collections.value) {
    const list = byParent.get(collection.parentKey) ?? [];
    list.push(collection);
    byParent.set(collection.parentKey, list);
  }
  const ordered: { collection: (typeof zotero.collections.value)[number]; depth: number }[] = [];
  const walk = (parent: string | null, depth: number): void => {
    for (const collection of byParent.get(parent) ?? []) {
      ordered.push({ collection, depth });
      walk(collection.key, depth + 1);
    }
  };
  walk(null, 0);
  return ordered;
});

const canExport = computed(
  () =>
    zotero.connectionState.value === 'ok' &&
    !zotero.needsZotero10.value &&
    zotero.selectedKey.value !== null &&
    !zotero.exporting.value
);

async function onSelect(key: string): Promise<void> {
  const collection = zotero.collections.value.find((c) => c.key === key);
  if (collection) {
    // The [connectionState, selectedKey] watch loads the preview.
    zotero.selectedKey.value = collection.key;
    zotero.selectedName.value = collection.name;
  }
}

async function onExport(): Promise<void> {
  await zotero.exportCollection(props.status, props.screeningErrorsOnly);
}

const progressPercent = computed(() => {
  const p = zotero.progress.value;
  if (!p || p.phase === 'authorize' || p.total === 0) return null;
  return Math.round((p.done / p.total) * 100);
});
</script>

<template>
  <div class="zep">
    <p class="zep__scope">Scope: {{ scopeLabel }}</p>

    <!-- Zotero < 10: writes need 10+ (import still works). Checked FIRST: the
         connector ping carries the version even while the local API pref is
         OFF, so a Zotero 9 with the API disabled sees this gate, not the
         enable-API card. -->
    <div v-if="zotero.needsZotero10.value" class="zep__card zep__card--info">
      Export to Zotero requires Zotero 10 or newer. Importing from Zotero still works.
    </div>

    <!-- Connection gate -->
    <div
      v-else-if="zotero.connectionState.value && zotero.connectionState.value !== 'ok'"
      class="zep__card zep__card--error"
    >
      {{ zotero.connectionMessage.value }}
      <button class="btn btn--outline" @click="zotero.openPanel()">Retry</button>
    </div>

    <template v-else>
      <div v-if="zotero.loadingDefaults.value" class="zep__state">
        <span class="spinner" /> Loading collections...
      </div>

      <template v-else>
        <label class="zep__field">
          Target collection
          <select
            class="zep__select"
            :value="zotero.selectedKey.value ?? ''"
            :disabled="zotero.exporting.value"
            @change="onSelect(($event.target as HTMLSelectElement).value)"
          >
            <option value="" disabled>Select a collection</option>
            <option v-for="entry in tree" :key="entry.collection.key" :value="entry.collection.key">
              {{ entry.depth > 0 ? '- '.repeat(entry.depth) : '' }}{{ entry.collection.name }}
            </option>
          </select>
        </label>

        <!-- Sync summary (DOI diff; nothing is written) -->
        <div v-if="zotero.previewLoading.value" class="zep__state">
          <span class="spinner" /> Comparing against Zotero...
        </div>
        <div v-else-if="zotero.preview.value" class="zep__summary">
          <span data-test="missing">{{ zotero.preview.value.missingCount }} to export</span>
          <span data-test="already"
            >{{ zotero.preview.value.alreadyPresentCount }} already present</span
          >
          <span data-test="no-doi"
            >{{ zotero.preview.value.noDoiCount }} without DOI (skipped)</span
          >
          <span v-if="zotero.includeFiles.value" data-test="files">
            {{ zotero.preview.value.fileCount }} full-text files
          </span>
        </div>

        <label class="zep__check">
          <input
            v-model="zotero.includeFiles.value"
            type="checkbox"
            :disabled="zotero.exporting.value"
          />
          Include full-text files
          <small>(uploads .pdf/.txt files to Zotero storage)</small>
        </label>

        <!-- Authorization phase: the Zotero dialog blocks; ask for Remember. -->
        <div
          v-if="zotero.authorizePhase.value"
          class="zep__card zep__card--info"
          data-test="authorize"
        >
          Zotero is asking for permission. Check Remember in the Zotero dialog so you are not asked
          every time.
        </div>

        <!-- Progress (items/files phases) -->
        <div v-if="zotero.progress.value && progressPercent !== null" class="zep__progress">
          <div class="zep__progress-bar">
            <div class="zep__progress-fill" :style="{ width: `${progressPercent}%` }" />
          </div>
          <span class="zep__progress-label" data-test="progress">
            {{ zotero.progress.value.phase }}: {{ zotero.progress.value.done }}/{{
              zotero.progress.value.total
            }}
          </span>
        </div>

        <div v-if="zotero.error.value" class="zep__card zep__card--error">
          {{ zotero.error.value }}
        </div>

        <!-- Result summary -->
        <div v-if="zotero.result.value" class="zep__card zep__card--ok" data-test="result">
          Exported {{ zotero.result.value.exportedCount }} to "{{
            zotero.result.value.collectionName
          }}".
          <span v-if="zotero.result.value.unchangedCount > 0">
            {{ zotero.result.value.unchangedCount }} already up to date.
          </span>
          <span v-if="zotero.result.value.alreadyPresentCount > 0">
            {{ zotero.result.value.alreadyPresentCount }} already present.
          </span>
          <span v-if="zotero.result.value.noDoiCount > 0">
            {{ zotero.result.value.noDoiCount }} skipped (no DOI).
          </span>
          <span v-if="zotero.includeFiles.value">
            Files: {{ zotero.result.value.fileAttachedCount }} attached,
            {{ zotero.result.value.fileFailedCount }} failed,
            {{ zotero.result.value.fileSkippedCount }} skipped.
          </span>
        </div>
      </template>
    </template>

    <div class="zep__actions">
      <button class="btn btn--outline" @click="emit('back')">Back</button>
      <button
        class="btn btn--primary"
        data-test="export-button"
        :disabled="!canExport"
        @click="onExport"
      >
        {{ zotero.exporting.value ? 'Exporting...' : 'Export' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.zep {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 12px);
}
.zep__scope {
  font-size: var(--font-size-caption, 13px);
  color: var(--color-on-surface-variant, #464555);
}
.zep__state {
  display: flex;
  align-items: center;
  gap: var(--space-2, 8px);
  color: var(--color-on-surface-variant, #464555);
  font-size: var(--font-size-caption, 13px);
}
.zep__field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 4px);
  font-size: var(--font-size-caption, 13px);
  color: var(--color-on-surface-variant, #464555);
}
.zep__select {
  padding: var(--space-2, 8px);
  border: 1px solid var(--color-outline, #777587);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-body, 14px);
  background: white;
}
.zep__summary {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2, 8px);
  font-size: var(--font-size-caption, 13px);
  color: var(--color-on-surface-variant, #464555);
}
.zep__check {
  display: flex;
  align-items: center;
  gap: var(--space-2, 8px);
  font-size: var(--font-size-caption, 13px);
}
.zep__check small {
  color: var(--color-on-surface-variant, #464555);
}
.zep__card {
  padding: var(--space-3, 12px);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 8px);
}
.zep__card--error {
  background-color: var(--color-error-container, #fdecea);
  color: var(--color-on-error-container, #5f2120);
}
.zep__card--info {
  background-color: var(--color-surface-container, #eae6f4);
  color: var(--color-on-surface, #1b1b24);
}
.zep__card--ok {
  background-color: #f0fdf4;
  color: #166534;
}
.zep__progress {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 4px);
}
.zep__progress-bar {
  height: 6px;
  background: var(--color-surface-container-high, #e6e0ee);
  border-radius: 3px;
  overflow: hidden;
}
.zep__progress-fill {
  height: 100%;
  background: var(--color-primary, #3525cd);
  transition: width 0.2s;
}
.zep__progress-label {
  font-size: var(--font-size-caption, 13px);
  color: var(--color-on-surface-variant, #464555);
}
.zep__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2, 8px);
}
.btn {
  padding: var(--space-2, 8px) var(--space-4, 16px);
  border-radius: var(--radius-default, 0.25rem);
  font-size: var(--font-size-caption, 13px);
  font-weight: var(--font-weight-semibold, 600);
  cursor: pointer;
}
.btn--primary {
  background-color: var(--color-primary, #3525cd);
  color: var(--color-on-primary, #ffffff);
}
.btn--outline {
  background: transparent;
  color: var(--color-on-surface-variant, #464555);
  border: 1px solid var(--color-outline, #777587);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
