<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { invoke } from '@tauri-apps/api/core';
import { useExport } from '@/composables/use-export';

const router = useRouter();
const { error, exportProject, importProject, resetProject } = useExport();

/** Open the Help Reference Backup & Restore section in a new tab/view. */
function openBackupHelp(): void {
  router.push('/help?tab=reference#ref-backup');
}

const showImportDialog = ref(false);
const showExportDialog = ref(false);
const showDeleteDialog = ref(false);
const deleteConfirmText = ref('');
const importFile = ref<File | null>(null);
/** Validation error shown inline when the user picks a non-backup file type.
 *  Distinct from `useExport().error`, which carries the backend import error. */
const importError = ref<string | null>(null);
/** Template ref for the hidden `<input type="file">` so the clear button can
 *  reset its `value` (otherwise re-selecting the same file won't re-fire
 *  `change`). */
const importFileInput = ref<HTMLInputElement | null>(null);

/** The effective Bango Documents directory path (storage root).
 * Shown in the Export dialog so the user knows what is NOT backed up. Fetched
 * from `get_storage_root`; `null` while loading or if the call fails. */
const bangoDocsDir = ref<string | null>(null);

interface StorageRootInfo {
  effectivePath: string;
  isCustom: boolean;
  defaultPath: string;
}

onMounted(async () => {
  try {
    const info = await invoke<StorageRootInfo>('get_storage_root');
    // The storage root is the Bango Documents directory directly (no
    // `fulltext` suffix to strip after the storage-root refactor).
    bangoDocsDir.value = info.effectivePath;
  } catch {
    // Non-fatal: the warning shows without the concrete path.
    bangoDocsDir.value = null;
  }
});

/** Accepted extensions for a Bango project backup file. The `accept` attribute
 *  on the input is advisory only (users can override it), so the handler also
 *  validates client-side and shows an inline error for mismatches. */
const ACCEPTED_BACKUP_EXTENSIONS = ['.bango.json', '.json'];

/** Returns true when `name` ends with one of the accepted backup extensions
 *  (case-insensitive). `.bango.json` is checked first so a file like
 *  `foo.bango.json` is not mis-matched on the bare `.json` branch. */
function isAcceptedBackupFile(name: string): boolean {
  const lower = name.toLowerCase();
  return ACCEPTED_BACKUP_EXTENSIONS.some((ext) => lower.endsWith(ext));
}

/** Handle a file selection from the hidden input. Resets the input's value
 *  immediately so selecting the same file twice still fires `change` (the
 *  browser skips the event when the value is unchanged). Validates the
 *  extension client-side because `accept` is advisory, not enforcing.
 *  @param event The DOM change event from the hidden `<input type="file">`. */
function handleImportFile(event: Event): void {
  const target = event.target as HTMLInputElement;
  const file = target.files?.[0] ?? null;
  // Reset regardless of outcome so a follow-up pick of the same file re-fires.
  target.value = '';
  if (!file) {
    importFile.value = null;
    importError.value = null;
    return;
  }
  if (!isAcceptedBackupFile(file.name)) {
    importFile.value = null;
    importError.value = 'Please select a .bango.json or .json file.';
    return;
  }
  importFile.value = file;
  importError.value = null;
}

/** Clear the current file selection, the inline error, and the underlying
 *  input element's value. Bound to the ✕ button next to the filename. */
function clearImportFile(): void {
  importFile.value = null;
  importError.value = null;
  if (importFileInput.value) {
    importFileInput.value.value = '';
  }
}

async function doImportProject(): Promise<void> {
  if (!importFile.value) return;
  await importProject(importFile.value);
  showImportDialog.value = false;
  importFile.value = null;
  // On success, importProject() triggers a full window.location.reload() so
  // ALL cached view state (keep-alive + module singletons) is wiped and the
  // app re-bootstraps against the freshly imported DB. No navigation needed.
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
    // On success, resetProject() triggers a full window.location.reload() so
    // ALL cached view state (keep-alive + module singletons) is wiped and the
    // app re-bootstraps against the freshly reset DB. No navigation needed.
  }
}
</script>

<template>
  <section class="settings-card">
    <h2 class="settings-card__title">
      <span class="material-symbols-outlined text-primary">settings_backup_restore</span>
      Project Management
    </h2>
    <p class="settings-card__desc">
      Start a new project, import a backup, export, or reset your data.
    </p>

    <!-- Info-box: surfaces the single-project "start fresh" workflow so the
         user understands Bango manages one project at a time and how to begin
         a new review. -->
    <div class="settings-card__info-box">
      <span class="material-symbols-outlined">tips_and_updates</span>
      <div>
        <p>
          Bango manages <strong>one project at a time</strong>. To start a new review, export a
          backup of your current project first, then use <strong>Delete All Data</strong> to begin
          fresh.
        </p>
        <button class="settings-card__learn-more" @click="openBackupHelp">
          <span class="material-symbols-outlined">menu_book</span>
          Learn more
        </button>
      </div>
    </div>

    <div class="settings-card__actions">
      <button class="btn btn--primary" @click="showDeleteDialog = true">
        <span class="material-symbols-outlined btn__icon">restart_alt</span>
        Start New Project
      </button>
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
          <label class="file-picker">
            <span class="material-symbols-outlined file-picker__icon">upload_file</span>
            <span v-if="!importFile" class="file-picker__placeholder">
              Select a .bango.json backup file
            </span>
            <span v-else class="file-picker__filename" :title="importFile.name">
              {{ importFile.name }}
            </span>
            <button
              v-if="importFile"
              type="button"
              class="file-picker__clear"
              aria-label="Clear selected file"
              title="Clear selected file"
              @click.stop.prevent="clearImportFile"
            >
              <span class="material-symbols-outlined">close</span>
            </button>
            <input
              ref="importFileInput"
              type="file"
              accept=".bango.json,.json"
              class="file-picker__input"
              aria-label="Project backup file"
              @change="handleImportFile"
            />
          </label>
          <p v-if="importError" class="file-picker__error" role="alert">{{ importError }}</p>
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

/* Info-box: indigo-tinted callout that explains the single-project model +
   the start-fresh workflow. Mirrors the `.dialog__info-box` shape but with a
   softer background so it sits in the card body, not a dialog. */
.settings-card__info-box {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
  padding: 0.75rem 1rem;
  margin-top: 0.75rem;
  margin-bottom: 1.25rem;
  background-color: #eef2ff;
  border: 1px solid #c7d2fe;
  border-radius: var(--radius-lg, 0.5rem);
  color: #312e81;
  font-size: 13px;
}

.settings-card__info-box .material-symbols-outlined {
  color: #4f46e5;
  margin-top: 2px;
  flex-shrink: 0;
}

.settings-card__info-box p {
  margin: 0;
  line-height: 18px;
}

.settings-card__info-box p + .settings-card__learn-more {
  margin-top: 0.5rem;
}

/* Inline "Learn more" text-link button - opens the Help Reference Backup &
   Restore section. Ghost style: no border, indigo text + icon, underlines on
   hover so it reads as a link, not a button. */
.settings-card__learn-more {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  background: transparent;
  border: none;
  padding: 0;
  color: #4f46e5;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  text-decoration: none;
  font-family: inherit;
  transition: text-decoration 0.15s;
}

.settings-card__learn-more:hover {
  text-decoration: underline;
}

.settings-card__learn-more .material-symbols-outlined {
  font-size: 16px;
}

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

/* Inline file picker styled to read as a single text input (mimics
   `.field__input`: same border, padding, radius, bg, font) so it sits in the
   same form family as the sibling text inputs. The whole box is a `<label>`
   wrapping a visually-hidden `<input type="file">`, so clicking anywhere opens
   the OS picker. The ✕ is pinned to the right edge via `margin-left: auto`. */
.file-picker {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  background-color: var(--color-surface-container-lowest, #ffffff);
  border: 1px solid var(--color-outline-variant, #c7c4d8);
  border-radius: var(--radius-lg, 0.5rem);
  padding: 0.625rem 1rem;
  font-size: 14px;
  line-height: 20px;
  color: var(--color-on-surface, #1b1b24);
  cursor: pointer;
  outline: none;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}

.file-picker:hover {
  border-color: var(--color-primary, #3525cd);
}

/* Focus ring mirrors `.field__input:focus`. Uses :focus-within so the hidden
   input's keyboard focus lights up the whole box. */
.file-picker:focus-within {
  border-color: var(--color-primary, #3525cd);
  box-shadow: 0 0 0 1px var(--color-primary, #3525cd);
}

.file-picker__icon {
  font-size: 18px;
  color: var(--color-outline, #777587);
  flex-shrink: 0;
}

.file-picker__placeholder {
  flex: 1;
  min-width: 0;
  color: var(--color-outline, #777587);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-picker__filename {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Visually hidden but still focusable and clickable via the wrapping label.
   Matches the `.dashboard__hidden-input` pattern. */
.file-picker__input {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

/* ✕ pinned to the right edge of the field. */
.file-picker__clear {
  margin-left: auto;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--color-outline, #777587);
  padding: var(--space-1);
  border-radius: var(--radius-sm, 0.25rem);
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.file-picker__clear .material-symbols-outlined {
  font-size: 18px;
}

.file-picker__clear:hover {
  background-color: #fef2f2;
  color: var(--color-error, #dc2626);
}

.file-picker__clear:focus-visible {
  outline: 2px solid var(--color-primary, #3525cd);
  outline-offset: 1px;
}

.file-picker__error {
  margin: 0;
  font-size: var(--font-size-caption, 13px);
  color: var(--color-error, #dc2626);
}
</style>
