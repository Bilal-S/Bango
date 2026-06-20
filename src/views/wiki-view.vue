<script setup lang="ts">
import { onMounted, onUnmounted, ref, computed, watch } from 'vue';
import { useRouter } from 'vue-router';
import { tauriCommand } from '@/composables/use-tauri-command';
import { useWiki } from '@/composables/use-wiki';
import type { WikiPageSummary } from '@/types/wiki';
import WikiToolbar from '@/components/wiki/wiki-toolbar.vue';
import WikiPageViewer from '@/components/wiki/wiki-page-viewer.vue';
import WikiPageEditor from '@/components/wiki/wiki-page-editor.vue';
import WikiGraphPanel from '@/components/wiki/wiki-graph-panel.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import { useArticleSearch } from '@/composables/use-article-search';
import { useToast } from '@/composables/use-toast';
import { open } from '@tauri-apps/plugin-dialog';

const router = useRouter();
const toast = useToast();
const {
  status,
  loading,
  error,
  refreshStatus,
  listPages,
  searchWiki,
  startProgressListener,
  stopProgressListener,
  exportAndIngest,
} = useWiki();

const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  moveArticle,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();

const showArticleDetail = ref(false);
const isArticleDetailFullScreen = ref(false);

const checkingLlm = ref(true);
const isLlmConfigured = ref(false);

const pages = ref<WikiPageSummary[]>([]);
const selectedSlug = ref<string | null>(null);
const mode = ref<'view' | 'edit'>('view');
const searchQuery = ref('');
const viewTab = ref<'pages' | 'graph'>('pages');
const collapsedSections = ref<Set<string>>(new Set());

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
  if (!q) return pages.value;
  return pages.value.filter(
    (p) =>
      p.title.toLowerCase().includes(q) ||
      p.summary.toLowerCase().includes(q) ||
      p.slug.toLowerCase().includes(q)
  );
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
};

onMounted(async () => {
  await Promise.all([checkLlmConfig(), refreshStatus(), startProgressListener()]);
  await loadPages();
  // Auto-ingest if wiki is stale (articles changed since last ingest).
  await autoIngestIfStale();
});

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
      selectedSlug.value = null;
      await loadPages();
      graphPanelRef.value?.refresh();
    } catch {
      // Non-fatal: user can manually rebuild via Re-scaffold.
    }
  }
}

async function checkLlmConfig(): Promise<void> {
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
      selectedSlug.value = pages.value[0]!.slug;
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
  selectedSlug.value = slug;
  mode.value = 'view';
  // Switch from graph to pages view so the user sees the opened page.
  viewTab.value = 'pages';
}

function navigateToPage(slug: string): void {
  selectedSlug.value = slug;
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

function onCloseArticleDetail(): void {
  showArticleDetail.value = false;
}

async function handleAttachFullText(articleId: string): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Documents', extensions: ['pdf', 'txt'] }],
    });
    if (!selected) return;
    toast.show('Importing full text…', 'info');
    await attachFullText(articleId, selected);
    toast.show('Full text attached successfully.', 'success');
  } catch {
    toast.show('Failed to attach full text', 'error');
  }
}

async function onIngested(): Promise<void> {
  await refreshStatus();
  selectedSlug.value = null;
  await loadPages();
  graphPanelRef.value?.refresh();
}

async function onDeleted(): Promise<void> {
  selectedSlug.value = null;
  pages.value = [];
  await refreshStatus();
}

onUnmounted(() => {
  stopProgressListener();
});

watch(searchQuery, async (q) => {
  if (q && q.trim().length >= 3) {
    try {
      const hits = await searchWiki(q.trim(), 50);
      void hits;
    } catch {
      // Fallback to client-side filtering.
    }
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
          <span class="material-symbols-outlined text-[32px]">auto_stories</span>
        </div>
        <h3 class="text-lg font-semibold text-slate-900 mb-2">Initialize Your Wiki</h3>
        <p class="text-sm text-slate-500 mb-6 leading-relaxed">
          Click <strong>Re-Scaffold</strong> in the toolbar to scaffold the
          <code class="wiki-code">wiki-root/</code> directory tree. Then use
          <strong>Add Documents</strong> to build the knowledge base.
        </p>
      </div>
    </div>

    <div
      v-show="viewTab === 'graph'"
      class="wiki-view__main flex-1 flex min-h-0"
      :class="{ hidden: viewTab !== 'graph' }"
    >
      <div class="flex-1 min-h-0">
        <WikiGraphPanel ref="graphPanelRef" @select-page="selectPage" />
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
            {{ filteredPages.length }} of {{ pages.length }} pages
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
              Use <strong>Re-Scaffold</strong> or <strong>Add Documents</strong> to generate wiki
              pages.
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
          <button
            class="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium text-indigo-600 hover:bg-indigo-50 rounded"
            @click="startEdit"
          >
            <span class="material-symbols-outlined text-[16px]">edit</span>
            Edit
          </button>
        </div>

        <WikiPageViewer
          v-if="mode === 'view'"
          :slug="selectedSlug"
          @navigate="navigateToPage"
          @view-article="viewArticle"
          @close="selectedSlug = null"
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
        @toggle-full-screen="isArticleDetailFullScreen = !isArticleDetailFullScreen"
        @update-notes="updateNotes"
        @update-tags="updateTags"
        @update-labels="updateLabels"
        @update-criteria="updateCriteria"
        @move-article="moveArticle"
        @attach-full-text="handleAttachFullText"
        @delete-full-text="deleteFullTextAttachment"
        @refresh-article="selectArticle"
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
