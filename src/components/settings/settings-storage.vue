<script setup lang="ts">
import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { folderLabelFromPath } from '@/utils/formatters';

// Storage root directory (Bango documents root; fulltext/, ris/, wiki-root/ derive from it).
interface StorageRootInfo {
  effectivePath: string;
  isCustom: boolean;
  defaultPath: string;
}
const storageInfo = ref<StorageRootInfo | null>(null);
const storageLoading = ref(false);
const storageError = ref<string | null>(null);

async function loadStorageInfo(): Promise<void> {
  try {
    storageInfo.value = await invoke<StorageRootInfo>('get_storage_root');
  } catch (e) {
    storageError.value = String(e);
  }
}

async function browseStorageDir(): Promise<void> {
  try {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      storageLoading.value = true;
      storageInfo.value = await invoke<StorageRootInfo>('set_storage_root', {
        path: selected,
      });
    }
  } catch (e) {
    storageError.value = String(e);
  } finally {
    storageLoading.value = false;
  }
}

async function resetStorageDir(): Promise<void> {
  storageLoading.value = true;
  try {
    storageInfo.value = await invoke<StorageRootInfo>('set_storage_root', { path: null });
  } catch (e) {
    storageError.value = String(e);
  } finally {
    storageLoading.value = false;
  }
}

/**
 * Display label for the storage tree root folder. Derived from the last
 * segment of `effectivePath` so a custom directory (e.g. `/data/my-research`)
 * shows `my-research/` instead of the hard-coded `Bango/`. Defaults to
 * `Bango/` when storage info hasn't loaded yet or the path is root-only.
 */
const rootFolderLabel = computed(() => folderLabelFromPath(storageInfo.value?.effectivePath ?? ''));

// Load storage info on mount.
loadStorageInfo();
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">folder_open</span>
      Storage
    </h2>
    <p class="settings-card__desc">
      Directory where Bango stores articles, full-text attachments, Citation Chaser output, and the
      LLM Wiki.
    </p>

    <div v-if="storageError" class="settings-card__status storage-error">
      {{ storageError }}
    </div>

    <div v-if="storageInfo" class="storage-dir">
      <div class="storage-dir__path-row">
        <code class="storage-dir__path">{{ storageInfo.effectivePath }}</code>
        <span v-if="storageInfo.isCustom" class="storage-dir__badge">Custom</span>
        <span v-else class="storage-dir__badge storage-dir__badge--default">Default</span>
      </div>

      <!-- Directory tree visual -->
      <div class="storage-tree">
        <div class="storage-tree__line storage-tree__line--root">
          <span class="material-symbols-outlined storage-tree__icon">folder</span>
          <code>{{ rootFolderLabel }}</code>
        </div>
        <div class="storage-tree__line">
          <span class="material-symbols-outlined storage-tree__icon">description</span>
          <span class="storage-tree__label"
            ><code>fulltext/</code> article PDFs + text extracts</span
          >
        </div>
        <div class="storage-tree__line">
          <span class="material-symbols-outlined storage-tree__icon">article</span>
          <span class="storage-tree__label"><code>ris/</code> Citations Files</span>
        </div>
        <div class="storage-tree__line">
          <span class="material-symbols-outlined storage-tree__icon">menu_book</span>
          <span class="storage-tree__label"><code>wiki-root/</code> LLM Wiki (Markdown)</span>
        </div>
      </div>

      <div class="settings-card__actions">
        <button class="btn btn--secondary" :disabled="storageLoading" @click="browseStorageDir">
          <span class="material-symbols-outlined btn__icon">folder</span>
          Browse...
        </button>
        <button
          v-if="storageInfo.isCustom"
          class="btn btn--secondary"
          :disabled="storageLoading"
          @click="resetStorageDir"
        >
          <span class="material-symbols-outlined btn__icon">undo</span>
          Reset to Default
        </button>
      </div>
      <p class="storage-dir__hint">
        Default: <code>{{ storageInfo.defaultPath }}</code>
      </p>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.storage-error {
  color: #991b1b;
  background-color: #fef2f2;
}

.storage-dir {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.storage-dir__path-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.storage-dir__path {
  flex: 1;
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 13px;
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 0.375rem;
  padding: 0.5rem 0.75rem;
  color: var(--color-on-surface, #1b1b24);
  word-break: break-all;
}

.storage-dir__badge {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  background-color: var(--color-primary, #3525cd);
  color: var(--color-on-primary, #ffffff);
  flex-shrink: 0;
}

.storage-dir__badge--default {
  background-color: var(--color-surface-variant, #e4e1ee);
  color: var(--color-on-surface-variant, #464555);
}

.storage-dir__hint {
  font-size: 12px;
  color: var(--color-outline, #777587);
  margin-top: 0.25rem;
}

.storage-dir__hint code {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 11px;
  background-color: var(--color-surface-container-low, #f5f2ff);
  padding: 0.0625rem 0.375rem;
  border-radius: 0.25rem;
}

/* ── Directory tree visual ── */
.storage-tree {
  margin-top: 0.75rem;
  padding: 0.75rem 1rem;
  background-color: var(--color-surface-container-low, #f5f2ff);
  border: 1px solid var(--color-surface-variant, #e4e1ee);
  border-radius: 0.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.375rem;
}

.storage-tree__line {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 13px;
  color: var(--color-on-surface, #1b1b24);
  padding-left: 0.25rem;
}

.storage-tree__line--root {
  font-weight: 600;
  padding-left: 0;
}

.storage-tree__line .storage-tree__icon {
  font-size: 18px;
  color: var(--color-primary, #3525cd);
  flex-shrink: 0;
}

.storage-tree__label {
  color: var(--color-on-surface-variant, #464555);
}

.storage-tree__label code,
.storage-tree__line code {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  font-size: 12px;
  color: var(--color-on-surface, #1b1b24);
}
</style>
