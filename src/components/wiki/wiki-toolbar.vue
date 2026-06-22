<script setup lang="ts">
import { ref, computed } from 'vue';
import { useRouter } from 'vue-router';
import { open } from '@tauri-apps/plugin-dialog';
import { useWiki } from '@/composables/use-wiki';
import { useToast } from '@/composables/use-toast';
import { useChatStore } from '@/stores/chat';
import type { WikiStatus } from '@/types/wiki';

const props = defineProps<{
  status: WikiStatus | null;
  /** Whether an LLM provider is configured. The Chat button is gated on this. */
  isLlmConfigured?: boolean;
}>();

const router = useRouter();
const chatStore = useChatStore();

const emit = defineEmits<{
  initialized: [];
  rawPrepared: [];
  ingested: [];
  deleted: [];
}>();

const {
  initWiki,
  addRawFile,
  addRawUrl,
  lintWiki,
  deleteWiki,
  rebuild,
  exportAndIngest,
  checkForUpdates,
  progress,
} = useWiki();
const toast = useToast();
const addingRaw = ref(false);
const showAddMenu = ref(false);
const showActionsMenu = ref(false);
const showWebDialog = ref(false);
const urlInput = ref('');
const fetchingUrl = ref(false);
const linting = ref(false);
const ingesting = ref(false);
const deleting = ref(false);
const rebuilding = ref(false);
const checkingUpdates = ref(false);
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
  rebuilding.value = true;
  try {
    const report = await rebuild();
    toast.show(
      `Rebuild complete: ${report.pagesWritten} pages written${report.errors.length > 0 ? `, ${report.errors.length} errors` : ''}.`,
      report.errors.length > 0 ? 'error' : 'success'
    );
    emit('ingested');
  } catch (e) {
    toast.show('Failed to rebuild wiki', 'error');
  } finally {
    rebuilding.value = false;
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
      toast.show('Documents added, but ingest failed. Click Rebuild Wiki to retry.', 'error');
    }
  } catch {
    toast.show('Failed to add files', 'error');
  } finally {
    addingRaw.value = false;
  }
}

/** Fetch URLs from the web dialog and add them as raw sources. */
async function handleAddFromWeb(): Promise<void> {
  const raw = urlInput.value.trim();
  if (!raw) return;

  // Parse URLs: split by newline and/or comma.
  const urls = raw
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0 && /^https?:\/\//i.test(s));

  if (urls.length === 0) {
    toast.show('No valid URLs found. URLs must start with http:// or https://', 'error');
    return;
  }

  fetchingUrl.value = true;
  let added = 0;
  let failed = 0;
  for (const url of urls) {
    try {
      await addRawUrl(url);
      added++;
    } catch {
      failed++;
    }
  }

  fetchingUrl.value = false;
  showWebDialog.value = false;
  urlInput.value = '';

  if (added === 0) {
    toast.show('Failed to fetch any URLs.', 'error');
    return;
  }

  toast.show(
    `Fetched ${added} page${added > 1 ? 's' : ''}${failed > 0 ? `, ${failed} failed` : ''}. Building wiki...`,
    'info'
  );
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
    toast.show('Pages fetched, but ingest failed. Click Rebuild Wiki to retry.', 'error');
  }
}

/** Run the lint engine (Health Check) and store the report for display. */
async function handleLint(): Promise<void> {
  linting.value = true;
  try {
    const report = await lintWiki();
    lintReport.value = report;
    const total = report.errors + report.warnings + report.infos;
    if (total === 0) {
      toast.show('Health check complete: clean. No issues found.', 'success');
    } else {
      const summary = `${report.errors} errors, ${report.warnings} warnings, ${report.infos} infos`;
      // Rebuild regenerates all pages via the LLM with the hardened prompt,
      // which fixes most broken-link / orphan issues. Recommend it whenever
      // any issue is present.
      toast.show(
        `Health check complete: ${summary}. Rebuild recommended.`,
        report.errors > 0 ? 'error' : 'warning'
      );
    }
  } catch (e) {
    toast.show('Failed to run health check', 'error');
  } finally {
    linting.value = false;
  }
}

/**
 * Manually trigger the on-demand drift check (bypasses the 30s debounce).
 * Detects external edits to wiki .md files and re-indexes them without an
 * LLM re-ingest. Emits `ingested` when pages were re-indexed so the parent
 * view refreshes the page list + graph.
 */
async function handleCheckUpdates(): Promise<void> {
  checkingUpdates.value = true;
  toast.show('Checking for Wiki updates...', 'info');
  try {
    const result = await checkForUpdates(true);
    if (!result) {
      // Debounced (shouldn't happen with force=true, but guard anyway).
      return;
    }
    if (result.rebuilt) {
      toast.show(`Wiki updated: ${result.pagesReindexed} pages re-indexed.`, 'success');
      emit('ingested');
    } else {
      toast.show('Wiki is up to date.', 'info', 1500);
    }
  } catch {
    toast.show('Failed to check for wiki updates', 'error');
  } finally {
    checkingUpdates.value = false;
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

/** Close the Actions menu when a menu item fires. */
function closeActionsMenu(): void {
  showActionsMenu.value = false;
}

/** The two dropdown menus on the toolbar (mutually exclusive). */
type MenuName = 'add' | 'actions';

/**
 * Toggle one dropdown menu open/closed and force the other closed, so only one
 * menu is ever visible at a time. Clicking the already-open menu closes it.
 */
function toggleMenu(menu: MenuName): void {
  if (menu === 'add') {
    showAddMenu.value = !showAddMenu.value;
    showActionsMenu.value = false;
  } else {
    showActionsMenu.value = !showActionsMenu.value;
    showAddMenu.value = false;
  }
}

/** The current rebuild/ingest state label shown in the Actions menu. */
function rebuildLabel(): string {
  return isInitialized() ? 'Rebuild Wiki' : 'Initialize Wiki';
}

/** Whether the Chat button should be enabled. Requires an LLM provider, an
 *  initialized wiki, and at least one generated page. */
const canChat = computed(() => {
  return (
    props.isLlmConfigured === true &&
    !!props.status?.initialized &&
    (props.status?.pageCount ?? 0) > 0
  );
});

/** Jump to the Chat view with Wiki mode pre-enabled so the user can chat
 *  against the wiki knowledge base (FTS5 RAG). Proactively sets
 *  `wikiReady=true` so the wiki toggle is visible immediately on arrival;
 *  `chat-view` reconfirms readiness on mount and downgrades only if the wiki
 *  is genuinely unavailable. */
function handleChat(): void {
  chatStore.setWikiReady(true);
  chatStore.setSource('wiki');
  void router.push('/chat');
}
</script>

<template>
  <div class="wiki-toolbar flex items-center gap-2 flex-wrap">
    <!-- LEFT: Add Documents + Actions -->
    <!-- Add Documents dropdown -->
    <div class="relative">
      <button
        class="wiki-toolbar__btn"
        :disabled="addingRaw || fetchingUrl || !isInitialized()"
        :title="
          isInitialized() ? 'Add documents from web or local drive' : 'Initialize the wiki first'
        "
        @click="toggleMenu('add')"
      >
        <span class="material-symbols-outlined text-[18px]">{{
          addingRaw || fetchingUrl ? 'hourglass_top' : 'attach_file_add'
        }}</span>
        <span>{{ addingRaw || fetchingUrl ? 'Adding...' : 'Add Documents' }}</span>
        <span class="material-symbols-outlined text-[16px]">arrow_drop_down</span>
      </button>
      <div v-if="showAddMenu" class="wiki-toolbar__menu" @click="showAddMenu = false">
        <button
          class="wiki-toolbar__menu-item"
          @click="
            () => {
              showAddMenu = false;
              showWebDialog = true;
            }
          "
        >
          <span class="material-symbols-outlined text-[16px] text-slate-500">language</span>
          From Web
        </button>
        <button
          class="wiki-toolbar__menu-item"
          @click="
            () => {
              showAddMenu = false;
              void handleAddRawFile();
            }
          "
        >
          <span class="material-symbols-outlined text-[16px] text-slate-500">folder_open</span>
          From Local Drive
        </button>
      </div>
    </div>

    <!-- Actions dropdown -->
    <div class="relative">
      <button
        class="wiki-toolbar__btn"
        :disabled="rebuilding"
        title="Rebuild, ingest, health check, or delete the wiki"
        @click="toggleMenu('actions')"
      >
        <span class="material-symbols-outlined text-[18px]">{{
          rebuilding ? 'hourglass_top' : 'play_circle'
        }}</span>
        <span>{{ rebuilding ? 'Working...' : 'Actions' }}</span>
        <span class="material-symbols-outlined text-[16px]">arrow_drop_down</span>
      </button>
      <div v-if="showActionsMenu" class="wiki-toolbar__menu" @click.self="showActionsMenu = false">
        <!-- Rebuild Wiki -->
        <button
          class="wiki-toolbar__menu-item"
          :disabled="rebuilding"
          title="Regenerate all wiki pages from raw sources — fixes broken links and stale content"
          @click="
            () => {
              closeActionsMenu();
              void handleInit();
            }
          "
        >
          <span class="material-symbols-outlined text-[16px] text-slate-500">sync</span>
          {{ rebuildLabel() }}
        </button>
        <!-- Ingest -->
        <button
          class="wiki-toolbar__menu-item"
          :disabled="ingesting || !isInitialized()"
          :title="
            isInitialized()
              ? 'Use the LLM to synthesize raw sources into wiki pages'
              : 'Initialize the wiki first'
          "
          @click="
            () => {
              closeActionsMenu();
              void handleIngest();
            }
          "
        >
          <span class="material-symbols-outlined text-[16px] text-slate-500">auto_awesome</span>
          {{ ingesting ? 'Ingesting...' : 'Ingest' }}
        </button>
        <!-- Health Check (was Lint) -->
        <button
          class="wiki-toolbar__menu-item"
          :disabled="linting || !isInitialized()"
          :title="
            isInitialized()
              ? 'Check for broken links, orphans, duplicates, missing frontmatter'
              : 'Initialize the wiki first'
          "
          @click="
            () => {
              closeActionsMenu();
              void handleLint();
            }
          "
        >
          <span class="material-symbols-outlined text-[16px] text-slate-500">fact_check</span>
          {{ linting ? 'Checking...' : 'Health Check' }}
        </button>
        <!-- Check for Updates: detect external edits to wiki pages + re-index -->
        <button
          class="wiki-toolbar__menu-item"
          :disabled="checkingUpdates || !isInitialized()"
          :title="
            isInitialized()
              ? 'Detect external edits to wiki pages and re-index them'
              : 'Initialize the wiki first'
          "
          @click="
            () => {
              closeActionsMenu();
              void handleCheckUpdates();
            }
          "
        >
          <span class="material-symbols-outlined text-[16px] text-slate-500">update</span>
          {{ checkingUpdates ? 'Checking...' : 'Check for Updates' }}
        </button>
        <!-- Divider -->
        <hr class="wiki-toolbar__menu-divider" />
        <!-- Delete Wiki -->
        <button
          class="wiki-toolbar__menu-item wiki-toolbar__menu-item--danger"
          :disabled="deleting || !isInitialized()"
          title="Delete all generated wiki pages (keeps raw sources)"
          @click="
            () => {
              closeActionsMenu();
              handleDeleteWiki();
            }
          "
        >
          <span class="material-symbols-outlined text-[16px]">delete_sweep</span>
          {{ deleting ? 'Deleting...' : 'Delete Wiki' }}
        </button>
      </div>
    </div>

    <!-- Chat: deep-link into the Chat view with Wiki mode pre-enabled. -->
    <button
      class="wiki-toolbar__btn"
      :disabled="!canChat"
      :title="
        canChat
          ? 'Chat with your wiki knowledge base (FTS5 search)'
          : 'Requires a configured LLM and an initialized wiki with pages'
      "
      @click="handleChat"
    >
      <span class="material-symbols-outlined text-[18px]">chat_add_on</span>
      <span>Chat</span>
    </button>

    <!-- Progress bar (when active, replaces stats on the left) -->
    <div v-if="progress" class="wiki-toolbar__progress">
      <div class="wiki-toolbar__progress-track">
        <div
          class="wiki-toolbar__progress-fill"
          :style="{ width: `${(progress.step / progress.totalSteps) * 100}%` }"
        ></div>
      </div>
      <span class="wiki-toolbar__progress-label">{{ progress.message }}</span>
    </div>

    <!-- Spacer pushes stats to the right -->
    <div v-if="!progress" class="flex-1"></div>

    <!-- RIGHT: Status pill + readiness gates -->
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

    <!-- From Web dialog -->
    <div v-if="showWebDialog" class="dialog-overlay" @click.self="showWebDialog = false">
      <div class="dialog">
        <h3 class="dialog__title">Add from Web</h3>
        <p class="dialog__desc">
          Enter one or more URLs (separated by new lines or commas). HTML content will be extracted
          and added as wiki sources.
        </p>
        <textarea
          v-model="urlInput"
          rows="5"
          class="field__input font-mono text-sm"
          placeholder="https://example.com/article1&#10;https://example.com/article2"
          :disabled="fetchingUrl"
        ></textarea>
        <div class="dialog__actions">
          <button class="btn btn--secondary" :disabled="fetchingUrl" @click="showWebDialog = false">
            Cancel
          </button>
          <button
            class="btn btn--primary"
            :disabled="fetchingUrl || !urlInput.trim()"
            @click="handleAddFromWeb"
          >
            <span v-if="fetchingUrl" class="wiki-toolbar__spinner"></span>
            {{ fetchingUrl ? 'Fetching...' : 'Fetch & Add' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Delete confirmation dialog -->
    <div v-if="showDeleteDialog" class="dialog-overlay" @click.self="showDeleteDialog = false">
      <div class="dialog dialog--danger">
        <h3 class="dialog__title">Delete Wiki?</h3>
        <div class="dialog__danger-box">
          <span class="material-symbols-outlined">warning</span>
          <span>
            This will permanently delete all generated wiki pages. Raw sources and templates are
            preserved. You can rebuild at any time with Rebuild Wiki.
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

.wiki-toolbar__btn--primary {
  background-color: rgb(99 102 241); /* indigo-600 */
  border-color: rgb(99 102 241);
  color: #fff;
}

.wiki-toolbar__btn--primary:hover:not(:disabled) {
  background-color: rgb(79 70 229); /* indigo-700 */
  color: #fff;
}

/* Shared dropdown menu styling */
.wiki-toolbar__menu {
  position: absolute;
  top: 100%;
  left: 0;
  margin-top: 0.25rem;
  background: #fff;
  border: 1px solid rgb(226 232 240);
  border-radius: 0.5rem;
  box-shadow:
    0 4px 6px -1px rgb(0 0 0 / 0.1),
    0 2px 4px -2px rgb(0 0 0 / 0.1);
  z-index: 50;
  min-width: 200px;
  padding: 0.25rem 0;
}

.wiki-toolbar__menu-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  text-align: left;
  padding: 0.5rem 0.75rem;
  font-size: 0.75rem;
  font-weight: 500;
  color: rgb(71 85 105); /* slate-600 */
  background: transparent;
  border: none;
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.wiki-toolbar__menu-item:hover:not(:disabled) {
  background-color: rgb(248 250 252); /* slate-50 */
  color: rgb(15 23 42); /* slate-900 */
}

.wiki-toolbar__menu-item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.wiki-toolbar__menu-item--danger {
  color: rgb(220 38 38); /* red-600 */
}

.wiki-toolbar__menu-item--danger:hover:not(:disabled) {
  background-color: rgb(254 242 242); /* red-50 */
}

.wiki-toolbar__menu-divider {
  border: none;
  border-top: 1px solid rgb(226 232 240);
  margin: 0.25rem 0;
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

.wiki-toolbar__spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid rgb(199 210 224);
  border-top-color: rgb(99 102 241);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
