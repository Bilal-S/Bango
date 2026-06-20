<script setup lang="ts">
import { ref } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { useWiki } from '@/composables/use-wiki';
import { useToast } from '@/composables/use-toast';
import type { WikiStatus } from '@/types/wiki';

const props = defineProps<{
  status: WikiStatus | null;
}>();

const emit = defineEmits<{
  initialized: [];
  rawPrepared: [];
  ingested: [];
  deleted: [];
}>();

const { initWiki, addRawFile, lintWiki, deleteWiki, rebuild, exportAndIngest, progress } =
  useWiki();
const toast = useToast();
const addingRaw = ref(false);
const linting = ref(false);
const ingesting = ref(false);
const deleting = ref(false);
const lintReport = ref<import('@/types/wiki').LintReport | null>(null);

async function handleInit(): Promise<void> {
  try {
    await initWiki();
    emit('initialized');
    // Auto-chain: run the full rebuild (export raw + ingest).
    await handleRebuild();
  } catch (e) {
    toast.show('Failed to initialize wiki', 'error');
  }
}

/** Full rebuild: scaffold + export raw + ingest in one async pipeline. */
async function handleRebuild(): Promise<void> {
  try {
    const report = await rebuild();
    toast.show(
      `Rebuild complete: ${report.pagesWritten} pages written${report.errors.length > 0 ? `, ${report.errors.length} errors` : ''}.`,
      report.errors.length > 0 ? 'error' : 'success'
    );
    emit('ingested');
  } catch (e) {
    toast.show('Failed to rebuild wiki', 'error');
  }
}

/** Open a file picker and add one or more selected files to `raw/`. */
async function handleAddRawFile(): Promise<void> {
  addingRaw.value = true;
  try {
    const selected = await open({
      multiple: true,
      filters: [
        {
          name: 'Documents',
          extensions: [
            'pdf',
            'txt',
            'text',
            'log',
            'html',
            'htm',
            'rtf',
            'csv',
            'md',
            'markdown',
            'json',
            'xml',
            'rs',
            'py',
            'js',
            'ts',
            'java',
            'c',
            'cpp',
            'go',
            'rb',
            'sh',
            'yml',
            'yaml',
            'toml',
            'ini',
            'cfg',
          ],
        },
      ],
    });
    if (!selected || (Array.isArray(selected) && selected.length === 0)) return;

    // Normalize to array — multiple: true returns string[] | null.
    const files: string[] = Array.isArray(selected) ? selected : [selected];

    let added = 0;
    let skipped = 0;
    for (const filePath of files) {
      try {
        await addRawFile(filePath);
        added++;
      } catch {
        // Unsupported file type or extraction error — skip.
        skipped++;
      }
    }

    if (added === 0) {
      toast.show('No supported files were added.', 'error');
      return;
    }

    const summary = `Added ${added} file${added > 1 ? 's' : ''}${skipped > 0 ? `, ${skipped} skipped` : ''}. Building wiki...`;
    toast.show(summary, 'info');
    emit('rawPrepared');
    // Auto-chain: export raw + ingest.
    try {
      const report = await exportAndIngest();
      toast.show(
        `Wiki rebuilt: ${report.pagesWritten} pages written${report.errors.length > 0 ? `, ${report.errors.length} errors` : ''}.`,
        report.errors.length > 0 ? 'error' : 'success'
      );
      emit('ingested');
    } catch {
      toast.show('Documents added, but ingest failed. Click Re-scaffold to retry.', 'error');
    }
  } catch {
    toast.show('Failed to add files', 'error');
  } finally {
    addingRaw.value = false;
  }
}

/** Run the lint engine and store the report for display. */
async function handleLint(): Promise<void> {
  linting.value = true;
  try {
    const report = await lintWiki();
    lintReport.value = report;
    const summary = `${report.errors} errors, ${report.warnings} warnings, ${report.infos} infos`;
    toast.show(`Lint complete: ${summary}.`, report.errors > 0 ? 'error' : 'success');
  } catch (e) {
    toast.show('Failed to lint wiki', 'error');
  } finally {
    linting.value = false;
  }
}

/** Run the LLM ingest: synthesize raw sources into wiki pages. */
async function handleIngest(): Promise<void> {
  // Check if ingestion is needed.
  if (!needsRefresh()) {
    toast.show('Wiki is up to date. No new documents to ingest.', 'info');
    return;
  }
  ingesting.value = true;
  try {
    const report = await exportAndIngest();
    toast.show(
      `Ingest complete: ${report.pagesWritten} pages written${report.errors.length > 0 ? `, ${report.errors.length} errors` : ''}.`,
      report.errors.length > 0 ? 'error' : 'success'
    );
    emit('ingested');
  } catch (e) {
    toast.show('Failed to ingest wiki', 'error');
  } finally {
    ingesting.value = false;
  }
}

/** Show the delete confirmation dialog. */
const showDeleteDialog = ref(false);

function handleDeleteWiki(): void {
  showDeleteDialog.value = true;
}

/** Actually delete the wiki after user confirms. */
async function confirmDeleteWiki(): Promise<void> {
  deleting.value = true;
  try {
    await deleteWiki();
    toast.show('Wiki deleted. Raw sources and templates are preserved.', 'success');
    emit('deleted');
  } catch {
    toast.show('Failed to delete wiki', 'error');
  } finally {
    deleting.value = false;
    showDeleteDialog.value = false;
  }
}

/** Whether the wiki has been scaffolded (AGENTS.md present). */
function isInitialized(): boolean {
  return props.status?.initialized === true;
}

/** Whether there are included articles to build a wiki from. */
function hasIncludedArticles(): boolean {
  return (props.status?.includedArticleCount ?? 0) > 0;
}

/** Whether the corpus changed since the last ingest (Phase 3 badge source). */
function needsRefresh(): boolean {
  return props.status?.needsRefresh === true;
}
</script>

<template>
  <div class="wiki-toolbar flex items-center gap-2 flex-wrap">
    <!-- Initialize / Re-scaffold -->
    <button
      class="wiki-toolbar__btn wiki-toolbar__btn--primary"
      :disabled="false"
      title="Create the wiki-root directory tree, AGENTS.md contract, and templates"
      @click="handleInit"
    >
      <span class="material-symbols-outlined text-[18px]">{{
        isInitialized() ? 'sync' : 'add_circle'
      }}</span>
      <span>{{ isInitialized() ? 'Re-scaffold' : 'Initialize Wiki' }}</span>
    </button>

    <!-- Status pill (hidden during progress) -->
    <div v-if="status && !progress" class="wiki-toolbar__pill" :title="`root: ${status.rootDir}`">
      <span class="material-symbols-outlined text-[14px]">folder</span>
      <span>{{ status.pageCount }} pages</span>
      <span class="wiki-toolbar__dot" aria-hidden="true">&middot;</span>
      <span>{{ status.rawCount }} raw</span>
      <span
        v-if="needsRefresh()"
        class="wiki-toolbar__badge"
        title="Included articles changed since the last ingest"
        >stale</span
      >
    </div>

    <!-- Readiness gate indicators (hidden during progress) -->
    <div v-if="status && !progress" class="wiki-toolbar__gates">
      <span class="wiki-toolbar__gate" :class="{ 'wiki-toolbar__gate--ok': hasIncludedArticles() }">
        <span class="material-symbols-outlined text-[14px]">{{
          hasIncludedArticles() ? 'check_circle' : 'radio_button_unchecked'
        }}</span>
        {{ status.includedArticleCount }} included
      </span>
    </div>

    <!-- Phase 2: Add a user file to raw/ -->
    <button
      class="wiki-toolbar__btn"
      :disabled="addingRaw || !isInitialized()"
      :title="
        isInitialized()
          ? 'Pick a PDF/TXT/HTML/RTF/CSV/MD/JSON/code file to add to raw/'
          : 'Initialize the wiki first'
      "
      @click="handleAddRawFile"
    >
      <span class="material-symbols-outlined text-[18px]">{{
        addingRaw ? 'hourglass_top' : 'attach_file_add'
      }}</span>
      <span>{{ addingRaw ? 'Adding...' : 'Add Documents' }}</span>
    </button>

    <!-- Placeholder slots for future phases.
         Ingest, Lint, Delete Wiki, Chat with Wiki, Open in Obsidian
         will be wired in Phases 3-5. The disabled state communicates scope. -->
    <!-- Phase 3/6: Ingest (LLM synthesize raw sources into wiki pages) -->
    <button
      class="wiki-toolbar__btn"
      :disabled="ingesting || !isInitialized()"
      :title="
        isInitialized()
          ? 'Use the LLM to synthesize raw sources into wiki pages'
          : 'Initialize the wiki first'
      "
      @click="handleIngest"
    >
      <span class="material-symbols-outlined text-[18px]">{{
        ingesting ? 'hourglass_top' : 'auto_awesome'
      }}</span>
      <span>{{ ingesting ? 'Ingesting...' : 'Ingest' }}</span>
    </button>
    <!-- Phase 4: Lint (deterministic, no LLM required) -->
    <button
      class="wiki-toolbar__btn"
      :disabled="linting || !isInitialized()"
      :title="
        isInitialized()
          ? 'Check for broken links, orphans, duplicates, missing frontmatter'
          : 'Initialize the wiki first'
      "
      @click="handleLint"
    >
      <span class="material-symbols-outlined text-[18px]">{{
        linting ? 'hourglass_top' : 'fact_check'
      }}</span>
      <span>{{ linting ? 'Linting...' : 'Lint' }}</span>
    </button>
    <!-- Progress bar (text overlaid on top) -->
    <div v-if="progress" class="wiki-toolbar__progress">
      <div class="wiki-toolbar__progress-track">
        <div
          class="wiki-toolbar__progress-fill"
          :style="{ width: `${(progress.step / progress.totalSteps) * 100}%` }"
        ></div>
      </div>
      <span class="wiki-toolbar__progress-label">{{ progress.message }}</span>
    </div>

    <!-- Delete Wiki -->
    <button
      class="wiki-toolbar__btn wiki-toolbar__btn--danger"
      :disabled="deleting || !isInitialized()"
      title="Delete all generated wiki pages (keeps raw sources)"
      @click="handleDeleteWiki"
    >
      <span class="material-symbols-outlined text-[18px]">{{
        deleting ? 'hourglass_top' : 'delete_sweep'
      }}</span>
      <span>{{ deleting ? 'Deleting...' : 'Delete Wiki' }}</span>
    </button>

    <!-- Delete confirmation dialog -->
    <div v-if="showDeleteDialog" class="dialog-overlay" @click.self="showDeleteDialog = false">
      <div class="dialog dialog--danger">
        <h3 class="dialog__title">Delete Wiki?</h3>
        <div class="dialog__danger-box">
          <span class="material-symbols-outlined">warning</span>
          <span>
            This will permanently delete all generated wiki pages. Raw sources and templates are
            preserved. You can rebuild at any time with Re-scaffold.
          </span>
        </div>
        <div class="dialog__actions">
          <button class="btn btn--secondary" @click="showDeleteDialog = false">Cancel</button>
          <button class="btn btn--danger" :disabled="deleting" @click="confirmDeleteWiki">
            {{ deleting ? 'Deleting...' : 'Delete Wiki' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wiki-toolbar {
  gap: 0.5rem;
}

.wiki-toolbar__btn {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 0.75rem;
  border-radius: 0.5rem;
  border: 1px solid rgb(226 232 240); /* slate-200 */
  background-color: #fff;
  color: rgb(71 85 105); /* slate-600 */
  font-size: 0.75rem;
  font-weight: 600;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.wiki-toolbar__btn:hover:not(:disabled) {
  background-color: rgb(248 250 252); /* slate-50 */
  color: rgb(15 23 42); /* slate-900 */
}

.wiki-toolbar__btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.wiki-toolbar__btn--danger {
  border-color: rgb(254 202 202);
  color: rgb(220 38 38);
}

.wiki-toolbar__btn--danger:hover:not(:disabled) {
  background-color: rgb(254 242 242);
}

.wiki-toolbar__btn--primary {
  background-color: rgb(99 102 241); /* indigo-600 */
  border-color: rgb(99 102 241);
  color: #fff;
}

.wiki-toolbar__btn--primary:hover:not(:disabled) {
  background-color: rgb(79 70 229); /* indigo-700 */
  color: #fff;
}

.wiki-toolbar__pill {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.25rem 0.625rem;
  border-radius: 9999px;
  background-color: rgb(241 245 249); /* slate-100 */
  border: 1px solid rgb(226 232 240);
  color: rgb(71 85 105);
  font-size: 0.7rem;
  font-weight: 500;
}

.wiki-toolbar__dot {
  color: rgb(203 213 225); /* slate-300 */
}

.wiki-toolbar__badge {
  margin-left: 0.25rem;
  padding: 0.0625rem 0.375rem;
  border-radius: 9999px;
  background-color: rgb(254 243 199); /* amber-100 */
  color: rgb(161 98 7); /* amber-800 */
  font-size: 0.625rem;
  font-weight: 700;
  text-transform: uppercase;
}

.wiki-toolbar__gates {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
}

.wiki-toolbar__gate {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.7rem;
  color: rgb(148 163 184); /* slate-400 */
}

.wiki-toolbar__gate--ok {
  color: rgb(22 163 74); /* green-600 */
}

.wiki-toolbar__progress {
  position: relative;
  flex: 1;
  min-width: 200px;
  max-width: 400px;
  height: 28px;
}

.wiki-toolbar__progress-track {
  position: absolute;
  inset: 0;
  background: rgb(226 232 240);
  border-radius: 9999px;
  overflow: hidden;
}

.wiki-toolbar__progress-fill {
  height: 100%;
  background: linear-gradient(90deg, rgb(99 102 241), rgb(139 92 246));
  border-radius: 9999px;
  transition: width 0.3s ease;
  animation: pulse 1.5s ease-in-out infinite;
}

.wiki-toolbar__progress-label {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  font-size: 0.7rem;
  font-weight: 600;
  color: rgb(15 23 42);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 95%;
  pointer-events: none;
  z-index: 1;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.7;
  }
}
</style>
