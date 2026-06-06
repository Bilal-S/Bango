<script setup lang="ts">
import { onMounted, ref, computed } from 'vue';
import { useRoute } from 'vue-router';
import { open } from '@tauri-apps/plugin-dialog';
import { useArticleSearch } from '@/composables/use-article-search';
import type { ArticleFilter } from '@/composables/use-article-search';
import { useToast } from '@/composables/use-toast';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';
import BulkActionBar from '@/components/bulk-action-bar.vue';
import ExportDialog from '@/components/export-dialog.vue';
import SuggestInput from '@/components/suggest-input.vue';

const route = useRoute();
const toast = useToast();

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
  selectedGlobalIndex,
  // Multi-select
  selectedIds,
  selectedCount,
  allSelected,
  someSelected,
  toggleSelect,
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

onMounted(() => {
  const status = typeof route.query.status === 'string' ? route.query.status : undefined;
  const tagsParam = typeof route.query.tags === 'string' ? route.query.tags.split(',') : undefined;
  const labelsParam =
    typeof route.query.labels === 'string' ? route.query.labels.split(',') : undefined;

  if (status || tagsParam || labelsParam) {
    void applyRouteParams({ status, tags: tagsParam, labels: labelsParam });
  } else {
    void search();
  }
});

const selectedId = computed(() => selectedArticle.value?.id ?? null);

const showExport = ref(false);
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
  closeDetail();
  isDetailFullScreen.value = false;
  localStorage.setItem('bango-detail-fullscreen', 'false');
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
  const { isLast } = await moveArticle(id, newStatus);
  const autoNavigate = localStorage.getItem('bango-auto-navigate-after-decision') !== 'false';
  if (isLast || !autoNavigate) {
    showDecisionNotification('Decision saved.', 'info');
  } else {
    showDecisionNotification('Decision saved. Moved to next article.', 'success');
  }
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
</script>

<template>
  <div class="h-full flex">
    <!-- Main content area -->
    <div v-show="!isDetailFullScreen" class="flex-1 p-container-padding overflow-y-auto">
      <!-- Header -->
      <div class="mb-6">
        <h1 class="page-title">Articles</h1>
      </div>

      <!-- Status Tabs -->
      <nav class="status-tabs flex items-center gap-6 mb-6 border-b border-slate-200">
        <button
          v-for="tab in STATUS_TABS"
          :key="tab"
          class="pb-3 text-sm font-medium transition-colors relative"
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

      <!-- Toolbar -->
      <ArticleToolbar
        :search-text="searchText"
        :show-filters="showFilters"
        :page-size="pageSize"
        :range-start="rangeStart"
        :range-end="rangeEnd"
        :total-count="resultCount"
        :is-filtered="isFiltered"
        :can-go-prev="canGoPrev"
        :can-go-next="canGoNext"
        @toggle-filters="toggleFilters"
        @update:search-text="searchText = $event"
        @search="executeToolbarSearch"
        @clear-search="clearSearch"
        @export-ris="showExport = true"
        @change-page-size="changePageSize"
        @go-prev="goToPage(currentPage - 1)"
        @go-next="goToPage(currentPage + 1)"
      />

      <!-- Filter Panel (collapsible) -->
      <ArticleFilterPanel
        v-if="showFilters"
        :filter="filter"
        :all-authors="allAuthors"
        :all-tags="allTags"
        :all-labels="allLabels"
        @apply="applyFilters"
        @clear="clearFilters"
        @close="toggleFilters"
        @update:filter="handleUpdateFilter"
      />

      <!-- Article Table -->
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
          @sort="toggleSort"
          @toggle-select="toggleSelect"
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
    </div>

    <!-- Export Dialog -->
    <ExportDialog v-if="showExport" @close="showExport = false" />

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
    />

    <!-- Bulk Action Bar -->
    <BulkActionBar
      :selected-count="selectedCount"
      @bulk-include="handleBulkInclude"
      @bulk-reject="handleBulkReject"
      @bulk-move-to-working="handleBulkMoveToWorking"
      @bulk-add-tag="openBulkTagDialog"
      @bulk-add-label="openBulkLabelDialog"
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
