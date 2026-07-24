<script setup lang="ts">
import { onMounted, onActivated, onUnmounted, ref, computed, watch, nextTick } from 'vue';
import { useRouter } from 'vue-router';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useWiki } from '@/composables/use-wiki';
import { useNavHistory } from '@/composables/use-nav-history';
import { isMacPlatform } from '@/utils/platform';
import { classifyWikiNavigationKey } from '@/utils/wiki-keyboard-navigation';
import { debounce } from '@/utils/debounce';
import type { WikiPageSummary } from '@/types/wiki';
import WikiToolbar from '@/components/wiki/wiki-toolbar.vue';
import WikiPageViewer from '@/components/wiki/wiki-page-viewer.vue';
import WikiPageEditor from '@/components/wiki/wiki-page-editor.vue';
import WikiGraphPanel from '@/components/wiki/wiki-graph-panel.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import { useArticleSearch } from '@/composables/use-article-search';
import { useScreening } from '@/composables/use-screening';
import { useToast } from '@/composables/use-toast';
import { useFullTextAttachment } from '@/composables/use-full-text-attachment';
import { useArticleDelete } from '@/composables/use-article-delete';
import { openPath } from '@tauri-apps/plugin-opener';

// Name the component so <keep-alive include="WikiView"> in app-shell.vue
// can cache it across navigation. Vue 3 <script setup> components are
// anonymous by default.
defineOptions({ name: 'WikiView' });

const router = useRouter();
const toast = useToast();
const {
  status,
  loading,
  error,
  initializing,
  refreshStatus,
  listPages,
  searchWiki,
  initWiki,
  startProgressListener,
  stopProgressListener,
  exportAndIngest,
  rebuild,
  checkForUpdates,
} = useWiki();

const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  refreshArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  updateMetadata,
  moveArticle,
  deleteArticle,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();
const { screenArticle } = useScreening();

const showArticleDetail = ref(false);
const isArticleDetailFullScreen = ref(false);

// Article delete UI orchestration is centralized in `useArticleDelete`
// (shared with the other detail-panel host views), mirroring
// `useFullTextAttachment`. The composable nulls `selectedArticle` (aliased as
// `detailArticle`), which reactively hides the panel via
// `v-if="showArticleDetail && detailArticle"`; the `onDeleted` hook resets the
// fullscreen flag + the local `showArticleDetail` gate.
const { handleDeleteArticle } = useArticleDelete({
  deleteArticle,
  onDeleted: () => {
    showArticleDetail.value = false;
    isArticleDetailFullScreen.value = false;
  },
});

const checkingLlm = ref(true);
const isLlmConfigured = ref(false);

const pages = ref<WikiPageSummary[]>([]);

// -- Full-text sidebar search ----------------------------------------------
// The sidebar search unions two sources:
// 1. Client-side filter (instant, per keystroke): title/summary/slug.
// 2. FTS5 BM25 search (debounced 250ms, backend): searches body content too.
// The index is kept fresh by the drift-detection feature.
const searchHits = ref<Set<string> | null>(null);
const isSearching = ref(false);

const runSearch = debounce(async (query: string) => {
  if (query !== searchQuery.value.trim()) return;
  isSearching.value = true;
  try {
    const hits = await searchWiki(query, 100);
    if (query === searchQuery.value.trim()) {
      searchHits.value = new Set(hits.map((h) => h.slug));
    }
  } catch {
    if (query === searchQuery.value.trim()) {
      searchHits.value = null;
    }
  } finally {
    if (query === searchQuery.value.trim()) {
      isSearching.value = false;
    }
  }
}, 250);

// Browser-like page navigation history (back/forward). `selectedSlug` is a
// read-only computed alias over the history's current entry so the template
// reads stay unchanged; all mutations go through `navHistory.navigate()` /
// `goBack()` / `goForward()` / `clear()`.
const navHistory = useNavHistory<string>();
const selectedSlug = navHistory.current;
const canGoBack = navHistory.canGoBack;
const canGoForward = navHistory.canGoForward;
// Platform detection is computed once (the OS does not change at runtime).
// `isMac` is passed to `classifyWikiNavigationKey` so the pure helper stays
// free of `navigator` reads and is trivially unit-testable.
const isMac = isMacPlatform();
// Platform-specific shortcut labels shown in the button tooltips.
const backShortcutLabel = isMac ? 'Cmd+[' : 'Alt+Left';
const forwardShortcutLabel = isMac ? 'Cmd+]' : 'Alt+Right';
const mode = ref<'view' | 'edit'>('view');
const searchQuery = ref('');
const viewTab = ref<'pages' | 'graph'>('pages');
// Authors is collapsed by default - it's a long list (one page per corpus
// author) that dominates the sidebar when expanded. Concepts / Sources /
// Methods / Synthesis start expanded.
const collapsedSections = ref<Set<string>>(new Set(['author']));

function toggleSection(type: string): void {
  const next = new Set(collapsedSections.value);
  if (next.has(type)) {
    next.delete(type);
  } else {
    next.add(type);
  }
  collapsedSections.value = next;
}
const graphPanelRef = ref<InstanceType<typeof WikiGraphPanel> | null>(null);

const needsSetup = computed(() => {
  if (!status.value) return false;
  return !status.value.initialized;
});

const hasPages = computed(() => pages.value.length > 0);

const filteredPages = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) {
    return pages.value;
  }
  // Client-side instant filter on metadata fields.
  const clientMatches = pages.value.filter(
    (p) =>
      p.title.toLowerCase().includes(q) ||
      p.summary.toLowerCase().includes(q) ||
      p.slug.toLowerCase().includes(q)
  );
  // Union with FTS5 body-content matches (when available).
  if (searchHits.value && searchHits.value.size > 0) {
    const clientSlugs = new Set(clientMatches.map((p) => p.slug));
    const ftsOnly = pages.value.filter(
      (p) => searchHits.value!.has(p.slug) && !clientSlugs.has(p.slug)
    );
    return [...clientMatches, ...ftsOnly];
  }
  return clientMatches;
});

const pagesByType = computed(() => {
  const groups: Record<string, WikiPageSummary[]> = {};
  for (const p of filteredPages.value) {
    const key = p.pageType || 'concept';
    if (!groups[key]) groups[key] = [];
    groups[key].push(p);
  }
  return groups;
});

const typeLabels: Record<string, string> = {
  concept: 'Concepts',
  author: 'Authors',
  method: 'Methods',
  synthesis: 'Synthesis',
  source: 'Sources',
};

onMounted(async () => {
  window.addEventListener('keydown', onKeyDown);
  await startProgressListener();
  await runReadinessChecks();
  // On-demand drift check: detect external edits to wiki .md files and
  // re-index transparently. Runs lock-free on the backend; the toast drives
  // the UX. Debounced 30s so navigation back-and-forth doesn't re-check.
  await checkForUpdatesOnMount();
});

/** Re-run all readiness checks (LLM config, wiki status, pages, stale ingest)
 *  whenever the user re-enters the Wiki view. This is critical because the
 *  view is keep-alive cached: `onMounted` only fires once for the component's
 *  lifetime, so without re-checking in `onActivated`, the empty-state gates
 *  (LLM configured, included articles > 0, wiki initialized) stay frozen at
 *  whatever value they had on first mount - e.g. the "LLM Provider Not
 *  Configured" card would persist even after the user configures an LLM in
 *  Settings and returns. All four calls are idempotent backend reads. */
async function runReadinessChecks(): Promise<void> {
  await Promise.all([checkLlmConfig(), refreshStatus()]);
  await loadPages();
  // Auto-ingest if wiki is stale (articles changed since last ingest).
  await autoIngestIfStale();
}

/** Re-check for external edits whenever the user re-enters the Wiki view
 *  (keep-alive re-activation). Respects the 30s debounce in useWiki so quick
 *  Wiki <-> Chat navigation does not re-check. */
onActivated(async () => {
  await runReadinessChecks();
  await checkForUpdatesOnMount();
  // If returning to the Graph tab, fix the stale Sigma canvas dimensions
  // that result from the container being display:none while away.
  if (viewTab.value === 'graph') {
    await nextTick();
    graphPanelRef.value?.handleResize();
  }
});

/**
 * Trigger the on-mount drift check. Silently skips when the wiki is not
 * initialized or when the debounce window is active. When drift is detected
 * and re-indexed, refreshes the page list + graph so the user sees the new
 * content immediately.
 */
async function checkForUpdatesOnMount(): Promise<void> {
  if (!status.value?.initialized) return;
  try {
    const result = await checkForUpdates(false);
    if (result?.rebuilt) {
      toast.show(`Wiki updated: ${result.pagesReindexed} pages re-indexed.`, 'success');
      await loadPages();
      graphPanelRef.value?.refresh();
    }
  } catch {
    // Non-fatal: the manual "Check for Updates" toolbar button is available.
  }
}

/**
 * Initialize + rebuild in one click. Used by the inline "Initialize Your Wiki"
 * empty-state card. Writes AGENTS.md (init), then runs the full pipeline
 * (scaffold + export raw + ingest). The native Tauri popup was removed in
 * favor of this in-page card so the prompt is non-blocking and visually
 * consistent with the other readiness gates (LLM config, no articles).
 */
async function initializeAndBuild(): Promise<void> {
  try {
    await initWiki();
    const report = await rebuild();
    toast.show(
      `Wiki ready: ${report.pagesWritten} pages written${report.errors.length > 0 ? `, ${report.errors.length} errors` : ''}.`,
      report.errors.length > 0 ? 'error' : 'success'
    );
    navHistory.clear();
    await refreshStatus();
    await loadPages();
    graphPanelRef.value?.refresh();
  } catch {
    toast.show('Failed to initialize and build wiki', 'error');
  }
}

/**
 * Platform-aware back/forward keyboard shortcuts (browser parity).
 * macOS: Cmd+[ / Cmd+] (also Cmd+Left / Cmd+Right).
 * Windows/Linux: Alt+Left / Alt+Right.
 *
 * Disabled while focus is inside an input/textarea/contenteditable, during
 * edit mode, on the Graph tab, or when there is no current page. Only calls
 * `preventDefault()` when the combo is actually handled so unrelated shortcuts
 * (e.g. Cmd+Left inside a text field) keep working.
 *
 * The platform/key/modifier decision matrix is delegated to the pure helper
 * {@link classifyWikiNavigationKey} (in `utils/wiki-keyboard-navigation.ts`)
 * which is exhaustively unit-tested without Vue or DOM dependencies. This
 * handler owns only the component-state guards + the `preventDefault` +
 * nav-history invocation.
 */
function onKeyDown(e: KeyboardEvent): void {
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) {
    return;
  }
  if (viewTab.value !== 'pages' || mode.value === 'edit' || !selectedSlug.value) return;

  const direction = classifyWikiNavigationKey(e, isMac);
  if (direction === 'back' && canGoBack.value) {
    e.preventDefault();
    navHistory.goBack();
  } else if (direction === 'forward' && canGoForward.value) {
    e.preventDefault();
    navHistory.goForward();
  }
}

/** Check if wiki needs refresh and auto-trigger export + ingest. */
async function autoIngestIfStale(): Promise<void> {
  if (
    status.value?.initialized &&
    isLlmConfigured.value &&
    status.value?.needsRefresh &&
    (status.value?.includedArticleCount ?? 0) > 0
  ) {
    try {
      const report = await exportAndIngest();
      toast.show(
        `Wiki auto-updated: ${report.pagesWritten} pages written.`,
        report.errors.length > 0 ? 'error' : 'success'
      );
      navHistory.clear();
      await loadPages();
      graphPanelRef.value?.refresh();
    } catch {
      // Non-fatal: user can manually rebuild via Actions -> Rebuild Wiki.
    }
  }
}

async function checkLlmConfig(): Promise<void> {
  // Reset the loading flag on every call (not just the first mount) so the
  // spinner shows while re-checking on keep-alive re-activation. Without
  // this, the stale `isLlmConfigured` value drives the empty-state gates
  // until the fresh fetch resolves.
  checkingLlm.value = true;
  try {
    isLlmConfigured.value = await tauriCommand<boolean>('has_llm_config');
  } catch {
    isLlmConfigured.value = false;
  } finally {
    checkingLlm.value = false;
  }
}

async function loadPages(): Promise<void> {
  if (!status.value?.initialized) return;
  try {
    pages.value = await listPages();
    if (pages.value.length > 0 && !selectedSlug.value) {
      navHistory.navigate(pages.value[0]!.slug);
    }
  } catch {
    pages.value = [];
  }
}

function goToSettings(): void {
  void router.push('/settings');
}

function goToArticles(): void {
  void router.push('/articles');
}

function selectPage(slug: string): void {
  navHistory.navigate(slug);
  mode.value = 'view';
  // Switch from graph to pages view so the user sees the opened page.
  viewTab.value = 'pages';
}

function navigateToPage(slug: string): void {
  navHistory.navigate(slug);
  mode.value = 'view';
}

function startEdit(): void {
  mode.value = 'edit';
}

function onSaved(): void {
  mode.value = 'view';
}

async function viewArticle(articleId: string): Promise<void> {
  await selectArticle(articleId);
  showArticleDetail.value = true;
}

/** Open an external document (uploaded via Add Documents) in the OS default
 * viewer. The slug resolves to a raw-file entry whose `sourceFile` is the
 * original filename (lives in `wiki-root/raw/`). Uses the same `openPath`
 * mechanism as the full-text reader's "Open Externally" button. */
async function openSource(slug: string): Promise<void> {
  try {
    const rawList =
      await tauriCommand<import('@/types/wiki').RawFileEntry[]>('wiki_list_raw_files');
    const entry = rawList.find((f) => f.slug === slug);
    if (!entry || !entry.sourceFile) {
      toast.show('Source file not found.', 'error');
      return;
    }
    const root = status.value?.rootDir;
    if (!root) {
      toast.show('Wiki root directory is not configured.', 'error');
      return;
    }
    // Build the absolute path to the original file inside wiki-root/raw/.
    const sep = root.includes('\\') ? '\\' : '/';
    const fullPath = `${root}${sep}raw${sep}${entry.sourceFile}`;
    await openPath(fullPath);
  } catch {
    toast.show('Failed to open source file.', 'error');
  }
}

function onCloseArticleDetail(): void {
  showArticleDetail.value = false;
}

// Full-text attach UI orchestration is centralized in
// `useFullTextAttachment` (shared with the other detail-panel host views).
const { handleAttachFullText } = useFullTextAttachment({ attachFullText });

async function onIngested(): Promise<void> {
  await refreshStatus();
  navHistory.clear();
  await loadPages();
  graphPanelRef.value?.refresh();
}

async function onDeleted(): Promise<void> {
  navHistory.clear();
  pages.value = [];
  await refreshStatus();
}

/** Reset the wiki view to its start state: clear the selected page + nav
 *  history, search query, graph tab, edit mode, and article-detail slide-over.
 *  Re-selects the first page. Does NOT re-fetch status or re-run the drift
 *  check (those are handled by onMounted / onActivated). Bound to the toolbar's
 *  reset button. */
function resetView(): void {
  navHistory.clear();
  searchQuery.value = '';
  searchHits.value = null;
  isSearching.value = false;
  viewTab.value = 'pages';
  mode.value = 'view';
  showArticleDetail.value = false;
  isArticleDetailFullScreen.value = false;
  collapsedSections.value = new Set(['author']);
  // Re-select the first page (mirrors loadPages initial selection).
  if (pages.value.length > 0) {
    navHistory.navigate(pages.value[0]!.slug);
  }
}

/** When the user switches to the Graph tab, focus the camera on the node for
 * the page they were viewing. Deferred via nextTick so the tab flip + any
 * ResizeObserver-deferred Sigma init completes first; the focusOnNode method
 * is defensive (no-op when Sigma isn't ready or the node has no coordinates). */
watch(viewTab, async (tab) => {
  if (tab === 'graph') {
    await nextTick();
    // Fix stale Sigma canvas dimensions that result from the container being
    // display:none while the Pages tab was active.
    graphPanelRef.value?.handleResize();
    if (selectedSlug.value) {
      graphPanelRef.value?.focusOnNode(selectedSlug.value);
    }
  }
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeyDown);
  stopProgressListener();
});

watch(searchQuery, (q) => {
  const trimmed = q.trim();
  if (trimmed.length >= 2) {
    searchHits.value = null;
    void runSearch(trimmed);
  } else {
    searchHits.value = null;
    isSearching.value = false;
  }
});
</script>

<template>
  <div class="wiki-view h-full flex flex-col overflow-hidden bg-slate-50/20">
    <header class="wiki-view__header px-container-padding py-4">
      <div class="wiki-view__header-row">
        <div>
          <h1 class="page-title">Wiki</h1>
          <p class="page-subtitle">
            Knowledge base built from your included articles and documents.
          </p>
        </div>
        <div v-if="status?.initialized" class="wiki-view__tabs">
          <button
            class="wiki-view__tab"
            :class="{ 'wiki-view__tab--active': viewTab === 'pages' }"
            @click="viewTab = 'pages'"
          >
            <span class="material-symbols-outlined text-[16px]">article</span>
            <span>Pages</span>
          </button>
          <button
            class="wiki-view__tab"
            :class="{ 'wiki-view__tab--active': viewTab === 'graph' }"
            @click="viewTab = 'graph'"
          >
            <span class="material-symbols-outlined text-[16px]">hub</span>
            <span>Graph</span>
          </button>
        </div>
      </div>
      <WikiToolbar
        v-if="status"
        :status="status"
        :is-llm-configured="isLlmConfigured"
        @reset="resetView"
        @initialized="
          async () => {
            await refreshStatus();
            await loadPages();
          }
        "
        @raw-prepared="refreshStatus"
        @ingested="onIngested"
        @deleted="onDeleted"
      />
    </header>

    <div v-if="checkingLlm || loading" class="flex-1 flex items-center justify-center">
      <div class="text-center">
        <div
          class="animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-600 mx-auto mb-4"
        ></div>
        <p class="text-sm text-slate-500">
          {{ checkingLlm ? 'Checking LLM configuration...' : 'Loading wiki status...' }}
        </p>
      </div>
    </div>

    <div v-else-if="!isLlmConfigured" class="flex-1 flex items-center justify-center p-6">
      <div
        class="max-w-md w-full bg-white rounded-2xl border border-slate-200 shadow-sm p-6 text-center animate-fade-in"
      >
        <div
          class="w-16 h-16 bg-amber-50 rounded-full flex items-center justify-center mx-auto mb-4 text-amber-600"
        >
          <span class="material-symbols-outlined text-[32px]">chat_error</span>
        </div>
        <h3 class="text-lg font-semibold text-slate-900 mb-2">LLM Provider Not Configured</h3>
        <p class="text-sm text-slate-500 mb-6 leading-relaxed">
          The Wiki uses the LLM to synthesize your included articles into a linked knowledge base.
          Please configure an LLM provider first.
        </p>
        <button
          class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-indigo-600 hover:bg-indigo-700 text-white font-medium shadow-sm transition-colors text-sm"
          @click="goToSettings"
        >
          <span class="material-symbols-outlined text-[18px]">settings</span>
          Configure LLM Settings
        </button>
      </div>
    </div>

    <div
      v-else-if="status && status.includedArticleCount === 0"
      class="flex-1 flex items-center justify-center p-6"
    >
      <div
        class="max-w-md w-full bg-white rounded-2xl border border-slate-200 shadow-sm p-6 text-center animate-fade-in"
      >
        <div
          class="w-16 h-16 bg-amber-50 rounded-full flex items-center justify-center mx-auto mb-4 text-amber-600"
        >
          <span class="material-symbols-outlined text-[32px]">inbox</span>
        </div>
        <h3 class="text-lg font-semibold text-slate-900 mb-2">No Included Articles</h3>
        <p class="text-sm text-slate-500 mb-6 leading-relaxed">
          The Wiki is built from your included article corpus. Move articles to the
          <strong>Included</strong> status to provide raw content.
        </p>
        <button
          class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-indigo-600 hover:bg-indigo-700 text-white font-medium shadow-sm transition-colors text-sm"
          @click="goToArticles"
        >
          <span class="material-symbols-outlined text-[18px]">description</span>
          Go to Articles
        </button>
      </div>
    </div>

    <div v-else-if="needsSetup" class="flex-1 flex items-center justify-center p-6">
      <div
        class="max-w-lg w-full bg-white rounded-2xl border border-slate-200 shadow-sm p-6 text-center animate-fade-in"
      >
        <div
          class="w-16 h-16 bg-indigo-50 rounded-full flex items-center justify-center mx-auto mb-4 text-indigo-600"
        >
          <span class="material-symbols-outlined text-[32px]">{{
            initializing ? 'progress_activity' : 'auto_stories'
          }}</span>
        </div>
        <h3 class="text-lg font-semibold text-slate-900 mb-2">Initialize Your Wiki</h3>
        <p class="text-sm text-slate-500 mb-6 leading-relaxed">
          The Wiki uses the LLM to synthesize your {{ status?.includedArticleCount ?? 0 }} included
          articles into a linked knowledge base. Initialize now to scaffold the
          <code class="wiki-code">wiki-root/</code> directory and generate pages.
        </p>
        <button
          class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-indigo-600 hover:bg-indigo-700 text-white font-medium shadow-sm transition-colors text-sm disabled:opacity-50 disabled:cursor-not-allowed"
          :disabled="initializing"
          @click="initializeAndBuild"
        >
          <span
            v-if="initializing"
            class="animate-spin rounded-full h-4 w-4 border-b-2 border-white"
          ></span>
          <span v-else class="material-symbols-outlined text-[18px]">auto_awesome</span>
          {{ initializing ? 'Building Wiki...' : 'Initialize & Build Wiki' }}
        </button>
      </div>
    </div>

    <div
      v-show="viewTab === 'graph'"
      class="wiki-view__main flex-1 flex min-h-0"
      :class="{ hidden: viewTab !== 'graph' }"
    >
      <div class="flex-1 min-h-0">
        <WikiGraphPanel ref="graphPanelRef" :focus-slug="selectedSlug" @select-page="selectPage" />
      </div>
    </div>

    <div
      v-show="viewTab === 'pages'"
      class="wiki-view__main flex-1 flex min-h-0"
      :class="{ hidden: viewTab !== 'pages' }"
    >
      <aside class="wiki-view__sidebar w-72 border-r border-slate-200 bg-white flex flex-col">
        <div class="p-3 border-b border-slate-200">
          <input
            v-model="searchQuery"
            class="wiki-search__input"
            type="search"
            placeholder="Search pages..."
          />
          <div class="mt-1 text-[10px] text-slate-400">
            {{ isSearching ? 'Searching...' : `${filteredPages.length} of ${pages.length} pages` }}
          </div>
        </div>
        <div class="flex-1 overflow-y-auto">
          <div v-if="filteredPages.length > 0">
            <div
              v-for="(groupPages, pageType) in pagesByType"
              :key="pageType"
              class="wiki-page-group"
            >
              <div class="wiki-page-group__header" @click="toggleSection(pageType)">
                <span class="material-symbols-outlined wiki-page-group__caret">{{
                  collapsedSections.has(pageType) ? 'chevron_right' : 'expand_more'
                }}</span>
                <span class="wiki-page-group__label">{{ typeLabels[pageType] || pageType }}</span>
                <span class="wiki-page-group__count">{{ groupPages.length }}</span>
              </div>
              <ul v-show="!collapsedSections.has(pageType)" class="wiki-page-list">
                <li
                  v-for="p in groupPages"
                  :key="p.slug"
                  class="wiki-page-list__item"
                  :class="{ 'wiki-page-list__item--active': selectedSlug === p.slug }"
                  @click="selectPage(p.slug)"
                >
                  <span class="wiki-page-list__title">{{ p.title }}</span>
                  <span v-if="p.summary" class="wiki-page-list__summary">{{ p.summary }}</span>
                  <span v-if="p.status === 'reviewed'" class="wiki-page-list__badge">reviewed</span>
                </li>
              </ul>
            </div>
          </div>
          <div v-else-if="!hasPages" class="p-4 text-center">
            <p class="text-xs text-slate-400 mb-2">No pages yet.</p>
            <p class="text-xs text-slate-400">
              Use <strong>Actions &rarr; Rebuild Wiki</strong> or <strong>Add Documents</strong> to
              generate wiki pages.
            </p>
          </div>
          <div v-else class="p-4 text-center">
            <p class="text-xs text-slate-400">No pages match "{{ searchQuery }}".</p>
          </div>
        </div>
      </aside>

      <div class="flex-1 flex flex-col min-h-0">
        <div
          v-if="selectedSlug && mode === 'view'"
          class="wiki-view__actionbar flex items-center justify-between px-4 py-2 border-b border-slate-200 bg-white"
        >
          <span class="text-xs text-slate-500 font-mono">{{ selectedSlug }}</span>
          <div class="flex items-center gap-1">
            <button
              class="wiki-nav-btn"
              :class="{ 'wiki-nav-btn--disabled': !canGoBack }"
              :disabled="!canGoBack"
              :title="`Back (${backShortcutLabel})`"
              :aria-label="`Back (${backShortcutLabel})`"
              @click="navHistory.goBack()"
            >
              <span class="material-symbols-outlined text-[16px]">arrow_back</span>
            </button>
            <button
              class="wiki-nav-btn"
              :class="{ 'wiki-nav-btn--disabled': !canGoForward }"
              :disabled="!canGoForward"
              :title="`Forward (${forwardShortcutLabel})`"
              :aria-label="`Forward (${forwardShortcutLabel})`"
              @click="navHistory.goForward()"
            >
              <span class="material-symbols-outlined text-[16px]">arrow_forward</span>
            </button>
            <button
              class="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium text-indigo-600 hover:bg-indigo-50 rounded"
              @click="startEdit"
            >
              <span class="material-symbols-outlined text-[16px]">edit</span>
              Edit
            </button>
          </div>
        </div>

        <WikiPageViewer
          v-if="mode === 'view'"
          :slug="selectedSlug"
          :highlight-query="searchQuery"
          @navigate="navigateToPage"
          @view-article="viewArticle"
          @open-source="openSource"
          @close="navHistory.clear()"
        />

        <WikiPageEditor v-else :slug="selectedSlug" @saved="onSaved" @cancel="mode = 'view'" />
      </div>
    </div>

    <p v-if="error" class="text-xs text-rose-500 px-4 py-2">{{ error }}</p>

    <!-- Article detail slide-over (opened from [^art-ref] source references). -->
    <Transition name="detail-slide">
      <ArticleDetailPanel
        v-if="showArticleDetail && detailArticle"
        :article="detailArticle"
        :audit-trail="detailAuditTrail"
        :has-previous="false"
        :has-next="false"
        :has-return-target="false"
        :full-screen="isArticleDetailFullScreen"
        :article-position="1"
        :article-total="1"
        @close="onCloseArticleDetail"
        @delete-article="handleDeleteArticle"
        @toggle-full-screen="isArticleDetailFullScreen = !isArticleDetailFullScreen"
        @update-notes="updateNotes"
        @update-tags="updateTags"
        @update-labels="updateLabels"
        @update-criteria="updateCriteria"
        @update-metadata="updateMetadata"
        @screen-article="screenArticle"
        @move-article="moveArticle"
        @attach-full-text="handleAttachFullText"
        @delete-full-text="deleteFullTextAttachment"
        @refresh-article="refreshArticle"
      />
    </Transition>
  </div>
</template>

<style scoped>
.wiki-view__header {
  border-bottom: 1px solid rgb(226 232 240);
}

.wiki-view__header-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}

.wiki-view__tabs {
  display: inline-flex;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.5rem;
  overflow: hidden;
}

.wiki-view__tab {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.375rem 0.75rem;
  font-size: 0.75rem;
  font-weight: 500;
  color: rgb(100 116 139);
  background: white;
  border: none;
  cursor: pointer;
  transition:
    background 0.15s,
    color 0.15s;
}

.wiki-view__tab:hover {
  background: rgb(248 250 252);
}

.wiki-view__tab--active {
  background: rgb(99 102 241);
  color: white;
}

.wiki-code {
  background-color: rgb(241 245 249);
  padding: 0.0625rem 0.3rem;
  border-radius: 0.25rem;
  font-family: monospace;
  font-size: 0.8em;
  color: rgb(71 85 105);
}

.wiki-search__input {
  width: 100%;
  padding: 0.375rem 0.625rem;
  border: 1px solid rgb(203 213 225);
  border-radius: 0.375rem;
  font-size: 0.75rem;
  outline: none;
}

.wiki-search__input:focus {
  border-color: rgb(99 102 241);
  box-shadow: 0 0 0 1px rgb(99 102 241);
}

.wiki-page-group {
  margin-bottom: 0.25rem;
}

.wiki-page-group__header {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.5rem 0.75rem 0.25rem;
  font-size: 0.65rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: rgb(100 116 139);
  background: rgb(248 250 252);
  position: sticky;
  top: 0;
  z-index: 1;
  cursor: pointer;
  user-select: none;
}

.wiki-page-group__header:hover {
  background: rgb(241 245 249);
}

.wiki-page-group__caret {
  font-size: 16px !important;
  color: rgb(148 163 184);
  flex-shrink: 0;
}

.wiki-page-group__label {
  flex: 1;
}

.wiki-page-group__count {
  background: rgb(226 232 240);
  color: rgb(71 85 105);
  padding: 0 0.375rem;
  border-radius: 9999px;
  font-size: 0.6rem;
}

.wiki-page-list {
  list-style: none;
  padding: 0;
  margin: 0;
}

.wiki-page-list__item {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  padding: 0.5rem 0.75rem;
  cursor: pointer;
  border-bottom: 1px solid rgb(241 245 249);
  position: relative;
}

.wiki-page-list__item:hover {
  background: rgb(248 250 252);
}

.wiki-page-list__item--active {
  background: rgb(238 242 255);
  border-left: 3px solid rgb(99 102 241);
}

.wiki-page-list__title {
  font-size: 0.8rem;
  font-weight: 600;
  color: rgb(15 23 42);
}

.wiki-page-list__summary {
  font-size: 0.65rem;
  color: rgb(100 116 139);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wiki-page-list__badge {
  position: absolute;
  top: 0.5rem;
  right: 0.75rem;
  font-size: 0.55rem;
  text-transform: uppercase;
  font-weight: 700;
  color: rgb(22 101 52);
  background: rgb(220 252 231);
  padding: 0.0625rem 0.3rem;
  border-radius: 9999px;
}

.wiki-view__actionbar {
  min-height: 2.25rem;
}

/* Back/Forward navigation icon buttons (left of Edit). */
.wiki-nav-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0.25rem;
  border: none;
  background: transparent;
  color: rgb(71 85 105);
  border-radius: 0.375rem;
  cursor: pointer;
  transition:
    background-color 0.15s,
    color 0.15s;
}

.wiki-nav-btn:hover:not(:disabled) {
  background-color: rgb(241 245 249);
  color: rgb(15 23 42);
}

.wiki-nav-btn--disabled,
.wiki-nav-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  pointer-events: none;
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.animate-fade-in {
  animation: fade-in 0.3s ease-out forwards;
}

/* Article detail panel slide-over: fly in from the right */
.detail-slide-enter-active,
.detail-slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}

.detail-slide-enter-from,
.detail-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}

/* Override the panel's flex behavior so it floats on the right */
.wiki-view :deep(.detail-panel) {
  position: fixed;
  top: 0;
  right: 0;
  height: 100vh;
  z-index: 50;
  box-shadow: -4px 0 24px rgb(0 0 0 / 12%);
  border-left: 1px solid rgb(226 232 240);
}

.wiki-view :deep(.detail-panel--fullscreen) {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  width: 100%;
  max-width: 100%;
  height: 100vh;
}
</style>
