<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { invoke } from '@tauri-apps/api/core';
import { useExport } from '@/composables/use-export';

const { error, exportProject, importProject, resetProject } = useExport();
const router = useRouter();

const showImportDialog = ref(false);
const showExportDialog = ref(false);
const showDeleteDialog = ref(false);
const deleteConfirmText = ref('');
const importFile = ref<File | null>(null);

/** The effective Bango Documents directory path (parent of the fulltext dir).
 * Shown in the Export dialog so the user knows what is NOT backed up. Fetched
 * from `get_fulltext_storage_dir`; `null` while loading or if the call fails. */
const bangoDocsDir = ref<string | null>(null);

interface StorageInfo {
  effectivePath: string;
  isCustom: boolean;
  defaultPath: string;
}

onMounted(async () => {
  try {
    const info = await invoke<StorageInfo>('get_fulltext_storage_dir');
    // The fulltext dir is `~/Documents/Bango/fulltext`; the Bango Documents
    // root (which also holds `wiki-root/`) is its parent. If the user set a
    // custom dir that does not end in `fulltext`, it is its own root.
    const path = info.effectivePath;
    bangoDocsDir.value = path.endsWith('fulltext')
      ? path.slice(0, Math.max(0, path.length - 'fulltext'.length)).replace(/[\\/]+$/, '')
      : path;
  } catch {
    // Non-fatal: the warning shows without the concrete path.
    bangoDocsDir.value = null;
  }
});

function handleImportFile(event: Event): void {
  const target = event.target as HTMLInputElement;
  if (target.files?.length) {
    importFile.value = target.files[0] ?? null;
  }
}

async function doImportProject(): Promise<void> {
  if (!importFile.value) return;
  await importProject(importFile.value);
  showImportDialog.value = false;
  importFile.value = null;
  // Navigate to dashboard so all views refresh with newly imported data
  if (!error.value) {
    router.push('/');
  }
}

async function doExportProject(): Promise<void> {
  await exportProject();
  showExportDialog.value = false;
}

async function doDeleteProject(): Promise<void> {
  if (deleteConfirmText.value.toUpperCase() !== 'DELETE') return;
  const success = await resetProject();
  if (success) {
    showDeleteDialog.value = false;
    deleteConfirmText.value = '';
    router.push('/');
  }
}
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">settings_backup_restore</span>
      Project Management
    </h2>
    <p class="settings-card__desc">Import, export, or reset your project data.</p>
    <div class="settings-card__actions">
      <button class="btn btn--secondary" @click="showImportDialog = true">
        <span class="material-symbols-outlined btn__icon">upload_file</span>
        Import Backup
      </button>
      <button class="btn btn--secondary" @click="showExportDialog = true">
        <span class="material-symbols-outlined btn__icon">download</span>
        Export Backup
      </button>
      <button class="btn btn--danger" @click="showDeleteDialog = true">
        <span class="material-symbols-outlined btn__icon">delete_forever</span>
        Delete All Data
      </button>
    </div>

    <!-- Import/Export Error Banner -->
    <div v-if="error" class="settings-card__error-banner">
      <span class="material-symbols-outlined">error</span>
      <p>{{ error }}</p>
    </div>

    <!-- Import Dialog -->
    <div v-if="showImportDialog" class="dialog-overlay" @click.self="showImportDialog = false">
      <div class="dialog">
        <h2>Import Project Backup</h2>
        <p class="dialog__desc">Select a <code>.bango.json</code> file to restore your project.</p>
        <div class="dialog__danger-box">
          <span class="material-symbols-outlined">warning</span>
          <p>
            <strong>All existing data will be deleted</strong> and replaced with the backup data.
            This action cannot be undone.
          </p>
        </div>
        <div class="field">
          <label class="field__label">Backup File</label>
          <input
            type="file"
            accept=".bango.json,.json"
            class="field__input"
            @change="handleImportFile"
          />
        </div>
        <div class="dialog__actions">
          <button class="btn btn--outline" @click="showImportDialog = false">Cancel</button>
          <button class="btn btn--primary" :disabled="!importFile" @click="doImportProject">
            Import
          </button>
        </div>
      </div>
    </div>

    <!-- Export Dialog -->
    <div v-if="showExportDialog" class="dialog-overlay" @click.self="showExportDialog = false">
      <div class="dialog">
        <h2>Export Project Backup</h2>
        <p class="dialog__desc">
          Export your project data to a <code>.bango.json</code> file. Note: API keys are NOT
          included in the backup.
        </p>
        <div class="dialog__info-box">
          <span class="material-symbols-outlined">info</span>
          <p>
            The backup includes articles, criteria, tags, labels, and references, but it does
            <strong>not</strong> back up the Bango Documents directory. Full-text PDFs and the
            generated Wiki are stored on disk and must be manually copied or moved to preserve
            them.<template v-if="bangoDocsDir"
              ><br />Documents directory: <code>{{ bangoDocsDir }}</code></template
            >
          </p>
        </div>
        <div class="dialog__actions">
          <button class="btn btn--outline" @click="showExportDialog = false">Cancel</button>
          <button class="btn btn--primary" @click="doExportProject">Export Backup</button>
        </div>
      </div>
    </div>

    <!-- Delete Confirmation Dialog -->
    <div v-if="showDeleteDialog" class="dialog-overlay" @click.self="showDeleteDialog = false">
      <div class="dialog dialog--danger">
        <h2>Delete All Project Data</h2>
        <div class="dialog__danger-box">
          <span class="material-symbols-outlined">warning</span>
          <p>
            This will permanently delete
            <strong>all articles, criteria, tags, labels, Wiki, and settings</strong>. This action
            cannot be undone.
          </p>
        </div>
        <div class="field">
          <label class="field__label">Type DELETE to confirm</label>
          <input
            v-model="deleteConfirmText"
            type="text"
            class="field__input"
            placeholder="DELETE"
          />
        </div>
        <div class="dialog__actions">
          <button
            class="btn btn--outline"
            @click="
              showDeleteDialog = false;
              deleteConfirmText = '';
            "
          >
            Cancel
          </button>
          <button
            class="btn btn--danger"
            :disabled="deleteConfirmText.toUpperCase() !== 'DELETE'"
            @click="doDeleteProject"
          >
            Delete Everything
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
@import './settings-card-shared.css';

.settings-card__error-banner {
  margin-top: 1rem;
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  background-color: #fef2f2;
  border: 1px solid #fecaca;
  border-radius: var(--radius-lg, 0.5rem);
  color: #991b1b;
  font-size: 13px;
}

.settings-card__error-banner .material-symbols-outlined {
  color: #dc2626;
  margin-top: 2px;
  flex-shrink: 0;
}

.settings-card__error-banner p {
  margin: 0;
  line-height: 18px;
  word-break: break-word;
}
</style>
