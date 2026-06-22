<script setup lang="ts">
import { onMounted, ref, computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { open } from '@tauri-apps/plugin-dialog';
import { useArticleSearch } from '@/composables/use-article-search';
import type { ArticleFilter } from '@/composables/use-article-search';
import { useToast } from '@/composables/use-toast';
import { requestArticleAiSummary } from '@/composables/use-ai-summary';
import { useFeatureFlags } from '@/composables/use-feature-flags';
import { useBatchReferenceScraping } from '@/composables/use-references';
import { useChatStore } from '@/stores/chat';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';
import BulkActionBar from '@/components/bulk-action-bar.vue';
import ExportDialog from '@/components/export-dialog.vue';
import SuggestInput from '@/components/suggest-input.vue';
import ReferencesView from '@/components/references-view.vue';
import BatchRefProgress from '@/components/batch-ref-progress.vue';

const route = useRoute();
const router = useRouter();
const toast = useToast();
const chatStore = useChatStore();

const {
  articles,
  loading,
  selectedArticle,
  auditTrail,
  showDetail,
  activeStatusTab,
  showFilters,
  sortColumn,
  sortDirection,
  filter,
  statusCounts,
  allAuthors,
  allTags,
  allLabels,
  STATUS_TABS,
  search,
  fetchCounts,
  selectArticle,
  hasPrevious,
  hasNext,
  navigatePrev,
  navigateNext,
  moveArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  closeDetail,
  setStatusTab,
  toggleSort,
  toggleFilters,
  applyFilters,
  clearFilters,
  applyRouteParams,
  currentPage,
  totalPages,
  canGoPrev,
  canGoNext,
  goToPage,
  searchText,
  activeTotalCount,
  isFiltered,
  resultCount,
  rangeStart,
  rangeEnd,
  pageSize,
  changePageSize,
  executeToolbarSearch,
  clearSearch,
  hasReturnTarget,
  navigateToArticle,
  returnToReferencePaperId,
  selectedGlobalIndex,
  // Multi-select
  selectedIds,
  selectedCount,
  allSelected,
  someSelected,
  toggleSelectRange,
  toggleSelectAll,
  clearSelection,
  // Bulk operations
  bulkUpdateStatus,
  bulkAddTag,
  bulkAddLabel,
  // Full text
  attachFullText,
  deleteFullTextAttachment,
  readFullTextContent,
} = useArticleSearch();

const activeReferencePaperId = ref<string | null>(null);

function handleNavigateToArticleWithRef(articleId: string, paperId?: string): void {
  if (paperId) {
    activeReferencePaperId.value = paperId;
  }
  navigateToArticle(articleId, paperId);
}

onMounted(() => {
  const status = typeof route.query.status === 'string' ? route.query.status : undefined;
  const tagsParam = typeof route.query.tags === 'string' ? route.query.tags.split(',') : undefined;
  const labelsParam =
    typeof route.query.labels === 'string' ? route.query.labels.split(',') : undefined;
  const yearFrom =
    typeof route.query.yearFrom === 'string' ? Number(route.query.yearFrom) : undefined;
  const yearTo = typeof route.query.yearTo === 'string' ? Number(route.query.yearTo) : undefined;
  const journal = typeof route.query.journal === 'string' ? route.query.journal : undefined;
  const author = typeof route.query.author === 'string' ? route.query.author : undefined;
  // filterCollapsed=1 → keep the filter panel collapsed (filters still applied)
  const filterCollapsed = route.query.filterCollapsed === '1';

  if (
    status ||
    tagsParam ||
    labelsParam ||
    (yearFrom !== undefined && Number.isFinite(yearFrom)) ||
    (yearTo !== undefined && Number.isFinite(yearTo)) ||
    journal ||
    author
  ) {
    void applyRouteParams({
      status,
      tags: tagsParam,
      labels: labelsParam,
      yearFrom,
      yearTo,
      journal,
      author,
      filterCollapsed,
    });
  } else {
    void search();
  }
});

/** Whether this article-list was opened via a deep-link from a bibliometric view. */
const fromBiblio = computed(
  () => route.query.from === 'timeline' || route.query.from === 'authors'
);

/** The biblio view name to return to ('timeline' or 'authors'). */
const biblioReturnName = computed(() => (route.query.from === 'timeline' ? 'timeline' : 'authors'));

/** The human-readable label for the back button. */
const biblioReturnLabel = computed(() =>
  route.query.from === 'timeline' ? 'Back to Timeline' : 'Back to Authors'
);

/** Return to the originating bibliometric view. */
function backToBiblio(): void {
  void router.push({ name: biblioReturnName.value });
}

const selectedId = computed(() => selectedArticle.value?.id ?? null);

const showExport = ref(false);
const pendingOpenReaderId = ref<string | null>(null);
const bulkTagDialogOpen = ref(false);
const bulkLabelDialogOpen = ref(false);
const bulkInputValue = ref('');
const isDetailFullScreen = ref(
  localStorage.getItem('bango-detail-fullscreen') === 'true' &&
    !!localStorage.getItem('bango-detail-fullscreen')
);
// Reset fullscreen state on fresh page load (no detail panel open)
// This prevents white screen when the user reloads without an article selected
setTimeout(() => {
  if (!showDetail.value) {
    isDetailFullScreen.value = false;
    localStorage.setItem('bango-detail-fullscreen', 'false');
  }
}, 0);

function toggleDetailFullScreen(): void {
  isDetailFullScreen.value = !isDetailFullScreen.value;
  localStorage.setItem('bango-detail-fullscreen', String(isDetailFullScreen.value));
}

/** Close detail panel and always reset fullscreen state to prevent white screen */
function handleCloseDetail(): void {
  const refPaperId = returnToReferencePaperId.value;
  closeDetail();
  if (refPaperId) {
    activeReferencePaperId.value = refPaperId;
  }
  isDetailFullScreen.value = false;
  localStorage.setItem('bango-detail-fullscreen', 'false');
}

/** Refresh status tab counts when references are updated */
async function handleReferencesUpdated(): Promise<void> {
  await fetchCounts();
}

// Inline decision notification state
const decisionMessage = ref('');
const decisionType = ref<'success' | 'info'>('success');
let decisionTimeout: ReturnType<typeof setTimeout> | null = null;

const STATUS_TAB_LABELS: Record<string, string> = {
  all: 'All',
  duplicate: 'Duplicates',
  working: 'Working',
  included: 'Included',
  rejected: 'Rejected',
  error: 'Errors',
  references: 'References',
};

const STATUS_TAB_TIPS: Record<string, string> = {
  all: 'All articles in our database',
  working: 'In-process articles to be reviewed',
  included: 'Articles included in research',
  rejected: 'Articles excluded from research',
  error: 'Articles with errors:check audit trail',
  duplicate: 'Duplicate articles',
  references: 'Browse all reference & citation papers',
};

function showDecisionNotification(message: string, type: 'success' | 'info'): void {
  if (decisionTimeout) clearTimeout(decisionTimeout);
  decisionMessage.value = message;
  decisionType.value = type;
  decisionTimeout = setTimeout(() => {
    decisionMessage.value = '';
  }, 2000);
}

async function handleMoveArticle(id: string, newStatus: string): Promise<void> {
  const { didNavigate } = await moveArticle(id, newStatus);
  if (didNavigate) {
    showDecisionNotification('Decision saved. Moved to next article.', 'success');
  } else {
    showDecisionNotification('Decision saved.', 'info');
  }
}

/** When a reference is promoted to an article, refresh the list and navigate to it */
async function handleArticlePromoted(articleId: string): Promise<void> {
  await search();
  selectArticle(articleId);
}

function handleUpdateFilter(key: keyof ArticleFilter, value: unknown): void {
  (filter as Record<string, unknown>)[key] = value;
}

// ── Bulk action handlers ──────────────────────────────────────────
async function handleBulkInclude(): Promise<void> {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  await bulkUpdateStatus(ids, 'included');
  toast.show(`${ids.length} article${ids.length > 1 ? 's' : ''} included`, 'success');
}

async function handleBulkReject(): Promise<void> {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  await bulkUpdateStatus(ids, 'rejected');
  toast.show(`${ids.length} article${ids.length > 1 ? 's' : ''} rejected`, 'success');
}

async function handleBulkMoveToWorking(): Promise<void> {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  await bulkUpdateStatus(ids, 'working');
  toast.show(`${ids.length} article${ids.length > 1 ? 's' : ''} moved to Working`, 'success');
}

function openBulkTagDialog(): void {
  bulkInputValue.value = '';
  bulkTagDialogOpen.value = true;
}

async function handleBulkAddTag(): Promise<void> {
  const name = bulkInputValue.value.trim();
  if (!name) return;
  const ids = Array.from(selectedIds.value);
  await bulkAddTag(ids, name);
  bulkTagDialogOpen.value = false;
  toast.show(`Tag "${name}" added to ${ids.length} article${ids.length > 1 ? 's' : ''}`, 'success');
}

function openBulkLabelDialog(): void {
  bulkInputValue.value = '';
  bulkLabelDialogOpen.value = true;
}

async function handleBulkAddLabel(): Promise<void> {
  const name = bulkInputValue.value.trim();
  if (!name) return;
  const ids = Array.from(selectedIds.value);
  await bulkAddLabel(ids, name);
  bulkLabelDialogOpen.value = false;
  toast.show(
    `Label "${name}" added to ${ids.length} article${ids.length > 1 ? 's' : ''}`,
    'success'
  );
}

function handleBulkAddToChat(): void {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  chatStore.clearSelectedArticles();
  for (const id of ids) {
    chatStore.addSelectedArticle(id);
  }
  toast.show(`Added ${ids.length} article${ids.length > 1 ? 's' : ''} to chat`, 'success');
  void router.push('/chat');
}

// ── Full text handlers ────────────────────────────────────────────
async function handleAttachFullText(articleId: string): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: 'Documents',
          extensions: ['pdf', 'txt'],
        },
      ],
    });
    if (!selected) return;
    toast.show('Importing full text…', 'info');
    await attachFullText(articleId, selected);
    toast.show('Full text attached successfully.', 'success');

    // Auto-summarize if Full Text Summaries preference is enabled
    if (localStorage.getItem('bango-full-text-summaries') === 'true') {
      const article = articles.value.find((a) => a.id === articleId);
      if (article) {
        // Pass a completion callback so the detail panel refreshes when the
        // summary finishes (even across navigation). Guarded so we don't
        // yank the user back if they navigated away during the LLM call.
        requestArticleAiSummary(articleId, article.title, handleAiSummaryComplete);
      }
    }
  } catch (e: unknown) {
    toast.show(`Failed to attach file: ${e instanceof Error ? e.message : String(e)}`, 'error');
  }
}

async function handleDeleteFullText(articleId: string): Promise<void> {
  try {
    await deleteFullTextAttachment(articleId);
    toast.show('Full text deleted.', 'success');
  } catch (e: unknown) {
    toast.show(
      `Failed to delete full text: ${e instanceof Error ? e.message : String(e)}`,
      'error'
    );
  }
}

async function handleReadFullText(articleId: string): Promise<string | null> {
  return await readFullTextContent(articleId);
}

/**
 * Completion callback for the auto-submitted AI summary (after a document
 * upload). Refreshes the detail panel only if the user is still viewing the
 * same article, so we don't yank them back if they navigated away during the
 * (long-running) LLM call.
 */
async function handleAiSummaryComplete(articleId: string): Promise<void> {
  if (selectedArticle.value?.id === articleId) {
    await selectArticle(articleId);
  }
}

function handleOpenReader(articleId: string): void {
  pendingOpenReaderId.value = articleId;
  selectArticle(articleId);
}

// ── Batch reference scraping ──────────────────────────────────
const { isPremium } = useFeatureFlags();
const {
  batchProgress,
  batchPercentage,
  startBatchScraping,
  cancelBatchScraping,
  resetBatchProgress,
} = useBatchReferenceScraping();

/** Only show batch button on Included tab when isPremium is on */
const showBatchRefScrape = computed(() => activeStatusTab.value === 'included' && isPremium.value);

/** Fetch all included articles for batch processing via search composable */
async function handleBatchScrapeRefs(): Promise<void> {
  const totalIncluded = statusCounts.value.included ?? 0;
  if (totalIncluded === 0) {
    toast.show('No included articles to process.', 'info');
    return;
  }

  // Temporarily set page size to fetch ALL included articles in one page
  const savedPageSize = pageSize.value;
  changePageSize(totalIncluded);
  await search();

  // Run batch on all fetched articles
  await startBatchScraping(articles.value, async () => {
    // Restore original page size and refresh
    changePageSize(savedPageSize);
    await handleReferencesUpdated();
    await search();
  });
}
</script>

<template>
  <div class="h-full flex">
    <!-- Main content area -->
    <div v-show="!isDetailFullScreen" class="flex-1 p-container-padding overflow-y-auto">
      <!-- Header -->
      <div class="mb-6 flex items-center gap-3">
        <button
          v-if="fromBiblio"
          class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-indigo-600 bg-indigo-50 hover:bg-indigo-100 rounded-lg cursor-pointer transition-colors"
          title="Return to the bibliometric view"
          @click="backToBiblio"
        >
          <span class="material-symbols-outlined text-sm">arrow_back</span>
          {{ biblioReturnLabel }}
        </button>
        <h1 class="page-title">Articles</h1>
      </div>

      <!-- Status Tabs -->
      <nav class="status-tabs flex items-center gap-6 mb-6 border-b border-slate-200">
        <button
          v-for="tab in STATUS_TABS"
          :key="tab"
          :title="STATUS_TAB_TIPS[tab]"
          class="pb-3 text-sm font-medium transition-colors relative cursor-default"
          :class="
            activeStatusTab === tab
              ? 'text-indigo-600 font-bold'
              : 'text-slate-500 hover:text-slate-900'
          "
          @click="setStatusTab(tab)"
        >
          <span>{{ STATUS_TAB_LABELS[tab] }}</span>
          <span class="ml-1.5 text-[11px] font-mono">
            {{ statusCounts[tab] ?? 0 }}
          </span>
          <!-- Active underline -->
          <span
            v-if="activeStatusTab === tab"
            class="absolute bottom-0 left-0 right-0 h-0.5 bg-indigo-600"
          />
        </button>
      </nav>

      <!-- References Tab Content -->
      <ReferencesView
        v-if="activeStatusTab === 'references'"
        :active-paper-id="activeReferencePaperId"
        @article-promoted="handleArticlePromoted"
        @navigate-to-article="handleNavigateToArticleWithRef"
        @update:active-paper-id="activeReferencePaperId = $event"
      />

      <!-- Toolbar (hidden on References tab) -->
      <ArticleToolbar
        v-if="activeStatusTab !== 'references'"
        :search-text="searchText"
        :show-filters="showFilters"
        :page-size="pageSize"
        :range-start="rangeStart"
        :range-end="rangeEnd"
        :total-count="resultCount"
        :is-filtered="isFiltered"
        :can-go-prev="canGoPrev"
        :can-go-next="canGoNext"
        :show-batch-ref-scrape="showBatchRefScrape"
        :is-batch-ref-running="batchProgress.isRunning"
        @toggle-filters="toggleFilters"
        @update:search-text="searchText = $event"
        @search="executeToolbarSearch"
        @clear-search="clearSearch"
        @export-ris="showExport = true"
        @change-page-size="changePageSize"
        @go-prev="goToPage(currentPage - 1)"
        @go-next="goToPage(currentPage + 1)"
        @batch-scrape-refs="handleBatchScrapeRefs"
        @clear-filters="
          clearSearch();
          clearFilters();
        "
      />

      <!-- Batch Reference Progress (below toolbar, visible when running or recently finished) -->
      <BatchRefProgress
        v-if="batchProgress.isRunning || batchProgress.completed > 0"
        :progress="batchProgress"
        :percentage="batchPercentage"
        :done="!batchProgress.isRunning && batchProgress.completed > 0"
        class="mb-4"
        @cancel="cancelBatchScraping"
        @close="resetBatchProgress"
      />

      <!-- Filter Panel (collapsible) -->
      <ArticleFilterPanel
        v-if="showFilters && activeStatusTab !== 'references'"
        :filter="filter"
        :all-authors="allAuthors"
        :all-tags="allTags"
        :all-labels="allLabels"
        @apply="applyFilters"
        @clear="clearFilters"
        @close="toggleFilters"
        @update:filter="handleUpdateFilter"
      />

      <!-- Article Table (hidden on References tab) -->
      <template v-if="activeStatusTab !== 'references'">
        <div v-if="loading" class="text-center py-16 text-slate-400 text-sm">Loading...</div>
        <template v-else>
          <ArticleTable
            :articles="articles"
            :selected-id="selectedId"
            :sort-column="sortColumn"
            :sort-direction="sortDirection"
            :selected-ids="selectedIds"
            :all-selected="allSelected"
            :some-selected="someSelected"
            @select="selectArticle"
            @open-reader="handleOpenReader"
            @sort="toggleSort"
            @toggle-select="toggleSelectRange"
            @toggle-select-all="toggleSelectAll"
          />

          <!-- Bottom pagination -->
          <div v-if="activeTotalCount > 0" class="flex items-center justify-center gap-2 mt-4 pb-4">
            <button
              class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
              :disabled="!canGoPrev"
              @click="goToPage(1)"
            >
              First
            </button>
            <button
              class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
              :disabled="!canGoPrev"
              @click="goToPage(currentPage - 1)"
            >
              &laquo; Prev
            </button>
            <span class="text-xs text-slate-600 min-w-[6rem] text-center">
              Page {{ currentPage }} of {{ totalPages }}
            </span>
            <button
              class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
              :disabled="!canGoNext"
              @click="goToPage(currentPage + 1)"
            >
              Next &raquo;
            </button>
            <button
              class="px-3 py-1.5 text-xs rounded-lg border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
              :disabled="!canGoNext"
              @click="goToPage(totalPages)"
            >
              Last
            </button>
          </div>
        </template>
      </template>
    </div>

    <!-- Export Dialog -->
    <ExportDialog
      v-if="showExport"
      :active-tab="activeStatusTab"
      :status-counts="statusCounts"
      :tab-label="STATUS_TAB_LABELS[activeStatusTab] ?? 'All'"
      @close="showExport = false"
    />

    <!-- Detail Panel -->
    <ArticleDetailPanel
      v-if="showDetail && selectedArticle"
      :article="selectedArticle"
      :audit-trail="auditTrail"
      :has-previous="hasPrevious"
      :has-next="hasNext"
      :has-return-target="hasReturnTarget"
      :full-screen="isDetailFullScreen"
      :article-position="selectedGlobalIndex"
      :article-total="activeTotalCount"
      :decision-message="decisionMessage"
      :decision-type="decisionType"
      :open-reader-id="pendingOpenReaderId"
      @reader-opened="pendingOpenReaderId = null"
      @close="handleCloseDetail"
      @navigate-prev="navigatePrev"
      @navigate-next="navigateNext"
      @move-article="handleMoveArticle"
      @update-notes="updateNotes"
      @update-tags="updateTags"
      @update-labels="updateLabels"
      @update-criteria="updateCriteria"
      @navigate-to-article="navigateToArticle"
      @toggle-full-screen="toggleDetailFullScreen"
      @attach-full-text="handleAttachFullText"
      @delete-full-text="handleDeleteFullText"
      @read-full-text="handleReadFullText"
      @refresh-article="selectArticle"
      @article-promoted="handleArticlePromoted"
      @references-updated="handleReferencesUpdated"
    />

    <!-- Bulk Action Bar -->
    <BulkActionBar
      :selected-count="selectedCount"
      @bulk-include="handleBulkInclude"
      @bulk-reject="handleBulkReject"
      @bulk-move-to-working="handleBulkMoveToWorking"
      @bulk-add-tag="openBulkTagDialog"
      @bulk-add-label="openBulkLabelDialog"
      @bulk-add-to-chat="handleBulkAddToChat"
      @clear-selection="clearSelection"
    />

    <!-- Bulk Tag Dialog -->
    <Teleport to="body">
      <div
        v-if="bulkTagDialogOpen"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        @click.self="bulkTagDialogOpen = false"
      >
        <div class="bg-white rounded-xl shadow-xl p-6 w-96 max-w-full">
          <h3 class="text-lg font-semibold mb-4">Add Tag to {{ selectedCount }} Articles</h3>
          <SuggestInput
            v-model="bulkInputValue"
            :suggestions="allTags"
            placeholder="Select or enter tag name"
            @enter="handleBulkAddTag"
          />
          <div class="flex justify-end gap-2 mt-4">
            <button
              class="px-4 py-2 text-sm rounded-lg border border-slate-300 hover:bg-slate-50"
              @click="bulkTagDialogOpen = false"
            >
              Cancel
            </button>
            <button
              class="px-4 py-2 text-sm rounded-lg bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-40"
              :disabled="!bulkInputValue.trim()"
              @click="handleBulkAddTag"
            >
              Add Tag
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Bulk Label Dialog -->
    <Teleport to="body">
      <div
        v-if="bulkLabelDialogOpen"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        @click.self="bulkLabelDialogOpen = false"
      >
        <div class="bg-white rounded-xl shadow-xl p-6 w-96 max-w-full">
          <h3 class="text-lg font-semibold mb-4">Add Label to {{ selectedCount }} Articles</h3>
          <SuggestInput
            v-model="bulkInputValue"
            :suggestions="allLabels"
            placeholder="Select or enter label name"
            @enter="handleBulkAddLabel"
          />
          <div class="flex justify-end gap-2 mt-4">
            <button
              class="px-4 py-2 text-sm rounded-lg border border-slate-300 hover:bg-slate-50"
              @click="bulkLabelDialogOpen = false"
            >
              Cancel
            </button>
            <button
              class="px-4 py-2 text-sm rounded-lg bg-purple-600 text-white hover:bg-purple-700 disabled:opacity-40"
              :disabled="!bulkInputValue.trim()"
              @click="handleBulkAddLabel"
            >
              Add Label
            </button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.status-tabs {
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
  scrollbar-width: none;
}

.status-tabs::-webkit-scrollbar {
  display: none;
}

@media (max-width: 767px) {
  .status-tabs {
    gap: 1rem;
  }
}
</style>
