<script setup lang="ts">
import { onMounted, onActivated, ref, computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useArticleSearch } from '@/composables/use-article-search';
import { useScreening } from '@/composables/use-screening';
import type { ArticleFilter } from '@/composables/use-article-search';
import { useToast } from '@/composables/use-toast';
import {
  requestArticleAiSummary,
  requestBulkArticleAiSummary,
  parseAiSummary,
  pendingSummaries,
} from '@/composables/use-ai-summary';
import { useFeatureFlags } from '@/composables/use-feature-flags';
import { useBatchReferenceScraping } from '@/composables/use-references';
import { useChatStore } from '@/stores/chat';
import { useFullTextAttachment } from '@/composables/use-full-text-attachment';
import { useArticleDelete } from '@/composables/use-article-delete';
import { useClearAiReasoning } from '@/composables/use-clear-ai-reasoning';
import { useExport } from '@/composables/use-export';
import { useLlmConfigured } from '@/composables/use-llm-configured';
import { resolveBiblioReturn } from '@/utils/biblio-links';
import {
  parseArticleRouteQuery,
  type ArticleRouteDeepLinkParams,
} from '@/utils/article-deep-links';
import { useArticleListKeyboard } from '@/composables/use-article-list-keyboard';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';
import BulkActionBar from '@/components/bulk-action-bar.vue';
import ExportDialog from '@/components/export-dialog.vue';
import SuggestInput from '@/components/suggest-input.vue';
import ReferencesView from '@/components/references-view.vue';
import OpenAlexSearch from '@/components/openalex-search.vue';
import BatchRefProgress from '@/components/batch-ref-progress.vue';

/* Named so <keep-alive :include="['WikiView', 'ArticleList']"> in app-shell
 * can cache across navigation. UI state survives navigation; `onActivated`
 * refreshes underlying data. */
defineOptions({ name: 'ArticleList' });

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
  deleteArticle,
  clearAiReasoning,
  refreshArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  updateMetadata,
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
  bulkRemoveTag,
  bulkRemoveLabel,
  // Full text
  attachFullText,
  deleteFullTextAttachment,
  readFullTextContent,
} = useArticleSearch();
const { screenArticle } = useScreening();

/* Canonical LLM-configured gate (see src/AGENTS.md Local Contracts). Passed
 * down to the BulkActionBar so its AI Summary action disables instantly when
 * the LLM config changes in Settings. */
const llmReady = useLlmConfigured();

const activeReferencePaperId = ref<string | null>(null);

function handleNavigateToArticleWithRef(articleId: string, paperId?: string): void {
  if (paperId) {
    activeReferencePaperId.value = paperId;
  }
  navigateToArticle(articleId, paperId);
}

/**
 * Apply deep-link params: filter params -> `applyRouteParams`, then select
 * articleId if provided. Returns true when any param was applied.
 */
function applyDeepLinkParams(params: ArticleRouteDeepLinkParams): boolean {
  const { hasFilterParams, articleId } = params;
  if (!hasFilterParams && !articleId) return false;
  if (hasFilterParams) {
    void applyRouteParams({
      status: params.status,
      tags: params.tagsParam,
      labels: params.labelsParam,
      yearFrom: params.yearFrom,
      yearTo: params.yearTo,
      journal: params.journal,
      author: params.author,
      filterCollapsed: params.filterCollapsed,
      resetFilters: params.resetFilters,
    }).then(() => {
      if (articleId) void selectArticle(articleId);
    });
  } else if (articleId) {
    // Only articleId deep-link (dashboard "Go to article" with no filter params)
    void selectArticle(articleId);
  }
  return true;
}

onMounted(() => {
  const params = parseArticleRouteQuery(route.query);
  const applied = applyDeepLinkParams(params);
  if (!applied) {
    // No deep-link params - run initial search
    void search().then(() => {
      if (params.articleId) void selectArticle(params.articleId);
    });
  }
});

/**
 * Keep-alive re-activation. Preserves UI state, refreshes underlying data.
 * Tab badges + rows re-fetch via `search()` (reuses preserved query). Open
 * detail + audit trail re-fetch. Deep-link params override preserved state.
 * First call (right after `onMounted`) is a no-op to avoid duplicate search.
 */
let isFirstActivation = true;

onActivated(() => {
  /* Skip first activation (fires right after onMounted). onMounted already
   * did the initial fetch + deep-link application. */
  if (isFirstActivation) {
    isFirstActivation = false;
    return;
  }

  const params = parseArticleRouteQuery(route.query);
  const articleIdDiffers = !!params.articleId && selectedArticle.value?.id !== params.articleId;

  /* Deep-link wins: re-apply filter params and/or select deep-linked article
   * when they differ from current state. Handles dashboard "Go to article",
   * biblio/tag/label deep-links arriving while view is cached. */
  if (params.hasFilterParams || articleIdDiffers) {
    applyDeepLinkParams(params);
    return;
  }

  /* Plain navigation (sidebar click on "Articles" with no query): preserve
   * all UI state and just refresh the data layer. Skip search() for
   * References/Search tabs - those child components own their data. */
  const tab = activeStatusTab.value;
  if (tab === 'references' || tab === 'search') {
    // Refresh the tab badges so they reflect imports / screening / bulk edits
    // that happened while away.
    void fetchCounts();
    return;
  }

  // Normal article tab: re-run the preserved query (refreshes rows + counts)
  // and refresh the open article detail + audit trail.
  void (async () => {
    await search();
    if (selectedArticle.value) {
      await selectArticle(selectedArticle.value.id);
    }
  })();
});

/** The return-target descriptor for the current origin, or null.
 * `fromBiblio` derives from this so the resolver runs once per `from`
 * change instead of being duplicated across two computeds. */
const biblioReturn = computed(() => resolveBiblioReturn(route.query.from as string));

/** Whether this article-list was opened via a deep-link from a bibliometric view. */
const fromBiblio = computed(() => biblioReturn.value !== null);

/** Return to the originating bibliometric view. */
function backToBiblio(): void {
  const target = biblioReturn.value;
  if (target) void router.push({ name: target.name });
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
  search: 'Search',
};

const STATUS_TAB_TIPS: Record<string, string> = {
  all: 'All articles in our database',
  working: 'In-process articles to be reviewed',
  included: 'Articles included in research',
  rejected: 'Articles excluded from research',
  error: 'Articles with errors:check audit trail',
  duplicate: 'Duplicate articles',
  references: 'Browse all reference & citation papers',
  search: 'Search the OpenAlex catalog',
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

/**
 * Non-navigating handler for the quick-add `(+)` on Articles-of-Interest
 * cards. Refreshes status-tab counts (e.g. Working) but does NOT open the
 * article detail panel - the card animates out within the References view.
 */
async function handleArticleAdded(): Promise<void> {
  await fetchCounts();
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

/* Shared bulk tag/label mutation: reads the dialog input, runs the IPC
 * mutation, closes the dialog, then toasts using the real affected count -
 * an info no-op message when nothing changed, or a success (add) /
 * warning (remove) action message with the exact affected/selected counts. */
async function runBulkTagLabelMutation(
  action: (ids: string[], name: string) => Promise<number>,
  kind: 'Tag' | 'Label',
  mode: 'add' | 'remove',
  closeDialog: () => void
): Promise<void> {
  const name = bulkInputValue.value.trim();
  if (!name) return;
  const ids = Array.from(selectedIds.value);
  const affected = await action(ids, name);
  closeDialog();
  const plural = ids.length > 1 ? 's' : '';
  if (affected === 0) {
    toast.show(
      mode === 'add'
        ? `${kind} "${name}" was already on all ${ids.length} selected article${plural}.`
        : `${kind} "${name}" was not present on any of the ${ids.length} selected article${plural}.`,
      'info'
    );
  } else {
    toast.show(
      mode === 'add'
        ? `${kind} "${name}" added to ${affected} of ${ids.length} selected article${plural}.`
        : `${kind} "${name}" removed from ${affected} of ${ids.length} selected article${plural}.`,
      mode === 'add' ? 'success' : 'warning'
    );
  }
}

function openBulkTagDialog(): void {
  bulkInputValue.value = '';
  bulkTagDialogOpen.value = true;
}

async function handleBulkAddTag(): Promise<void> {
  await runBulkTagLabelMutation(bulkAddTag, 'Tag', 'add', () => {
    bulkTagDialogOpen.value = false;
  });
}

/* "Remove Tag" button in the Change Tag dialog; toast semantics in
 * `runBulkTagLabelMutation`. */
async function handleBulkRemoveTag(): Promise<void> {
  await runBulkTagLabelMutation(bulkRemoveTag, 'Tag', 'remove', () => {
    bulkTagDialogOpen.value = false;
  });
}

function openBulkLabelDialog(): void {
  bulkInputValue.value = '';
  bulkLabelDialogOpen.value = true;
}

async function handleBulkAddLabel(): Promise<void> {
  await runBulkTagLabelMutation(bulkAddLabel, 'Label', 'add', () => {
    bulkLabelDialogOpen.value = false;
  });
}

/* "Remove Label" button in the Change Label dialog; toast semantics in
 * `runBulkTagLabelMutation`. */
async function handleBulkRemoveLabel(): Promise<void> {
  await runBulkTagLabelMutation(bulkRemoveLabel, 'Label', 'remove', () => {
    bulkLabelDialogOpen.value = false;
  });
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

/* Bulk export: sole entry point for "export selected." Opens OS save dialog
 * and writes RIS for exactly checked articles. Toolbar Export = tab/status,
 * bulk bar = selected rows only. */
const { exportRisForIds, error: exportError } = useExport();

async function handleBulkExport(): Promise<void> {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  const ok = await exportRisForIds(ids);
  if (ok) {
    toast.show(`Exported ${ids.length} article${ids.length > 1 ? 's' : ''} to RIS`, 'success');
    // Selection clears only on success; cancel (dismissed save dialog) and
    // failures keep the checked rows so the user can retry.
    clearSelection();
  } else if (exportError.value) {
    // A real failure surfaces via the composable's error ref. Cancel (user
    // dismissed the save dialog) leaves error null and stays silent.
    toast.show(`Export failed: ${exportError.value}`, 'error');
  }
}

/* Bulk AI Summary: enqueue every selected article that is actually
 * summarizable - full text attached, no stored summary yet, and not already
 * queued. Eligibility mirrors the detail panel's `canRequestAiSummary` so the
 * batch path and the per-article button agree on what "needs a summary"
 * means. Skips are reported with exact counts (runBulkTagLabelMutation
 * precedent); the submit toast comes from the composable. The selection is
 * cleared once the batch is accepted - completion is async and can run for
 * minutes, so the user should not stay locked into a stale selection. */
function handleBulkAiSummary(): void {
  let withoutFullText = 0;
  let alreadySummarized = 0;
  const eligible: string[] = [];

  for (const id of selectedIds.value) {
    const article = articles.value.find((a) => a.id === id);
    if (!article) continue;
    if (!article.hasFullText || !article.fullText) {
      withoutFullText += 1;
      continue;
    }
    if (parseAiSummary(article.fullTextAiSummary) !== null || pendingSummaries.value.has(id)) {
      alreadySummarized += 1;
      continue;
    }
    eligible.push(id);
  }

  if (eligible.length === 0) {
    toast.show('No selected articles are eligible for an AI summary.', 'info');
    return;
  }
  requestBulkArticleAiSummary(eligible);

  const skipped = withoutFullText + alreadySummarized;
  if (skipped > 0) {
    const parts: string[] = [];
    if (withoutFullText > 0) parts.push(`${withoutFullText} without full text`);
    if (alreadySummarized > 0) parts.push(`${alreadySummarized} already summarized`);
    toast.show(
      `Skipped ${skipped} of ${selectedIds.value.size} selected: ${parts.join(', ')}.`,
      'info'
    );
  }

  // Clear at submit time (after the skip toast computed its totals): the
  // batch is accepted, and nothing-eligible above keeps the selection.
  clearSelection();
}

/* Full text orchestration centralized in `useFullTextAttachment`; auto-summarize
 * branch preserved via `onAttached` hook. */
const { handleAttachFullText } = useFullTextAttachment({
  attachFullText,
  onAttached: (articleId) => {
    // Auto-summarize if Full Text Summaries preference is enabled
    if (localStorage.getItem('bango-full-text-summaries') === 'true') {
      const article = articles.value.find((a) => a.id === articleId);
      if (article) {
        /* Pass completion callback so detail panel refreshes when summary
         * finishes. Guarded to avoid yanking user back if they navigated away. */
        requestArticleAiSummary(articleId, article.title, handleAiSummaryComplete);
      }
    }
  },
});

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

/* Article delete orchestration centralized in `useArticleDelete`. Composable
 * owns toast + post-delete hook; `useArticleSearch.deleteArticle` owns IPC +
 * list/panel teardown. */
const { handleDeleteArticle } = useArticleDelete({
  deleteArticle,
  onDeleted: () => {
    // The panel is gone; reset the fullscreen flag so a fresh open starts clean.
    isDetailFullScreen.value = false;
    localStorage.setItem('bango-detail-fullscreen', 'false');
  },
});

/* AI-reasoning clear orchestration centralized in `useClearAiReasoning`.
 * Composable owns toast; `useArticleSearch.clearAiReasoning` owns IPC + refresh. */
const { handleClearAiReasoning } = useClearAiReasoning({ clearAiReasoning });

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

/* Keyboard navigation: context-dependent arrow-key shortcuts, extracted to
 * `use-article-list-keyboard` (refactor1 T4.2). Listener lifecycle is owned by
 * the composable (onActivated / onDeactivated / onUnmounted) because this view
 * is keep-alive cached: the shortcuts must only fire while Articles is the
 * active view. */
const articleTableRef = ref<InstanceType<typeof ArticleTable> | null>(null);

useArticleListKeyboard({
  showDetail,
  selectedArticle,
  activeStatusTab,
  hasPrevious,
  hasNext,
  navigatePrev,
  navigateNext,
  articleTableRef,
});
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
          {{ biblioReturn?.label }}
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
          <span v-if="tab !== 'search'" class="ml-1.5 text-[11px] font-mono">
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
        @article-added="handleArticleAdded"
        @navigate-to-article="handleNavigateToArticleWithRef"
        @update:active-paper-id="activeReferencePaperId = $event"
      />

      <!-- Search Tab Content (OpenAlex) -->
      <OpenAlexSearch v-if="activeStatusTab === 'search'" @imported="handleReferencesUpdated" />

      <!-- Toolbar (hidden on References + Search tabs) -->
      <ArticleToolbar
        v-if="activeStatusTab !== 'references' && activeStatusTab !== 'search'"
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

      <!-- Filter Panel (collapsible, hidden on References + Search tabs) -->
      <ArticleFilterPanel
        v-if="showFilters && activeStatusTab !== 'references' && activeStatusTab !== 'search'"
        :filter="filter"
        :all-authors="allAuthors"
        :all-tags="allTags"
        :all-labels="allLabels"
        :result-count="resultCount"
        :is-filtered="isFiltered"
        @apply="applyFilters"
        @clear="clearFilters"
        @close="toggleFilters"
        @update:filter="handleUpdateFilter"
      />

      <!-- Article Table (hidden on References + Search tabs) -->
      <template v-if="activeStatusTab !== 'references' && activeStatusTab !== 'search'">
        <div v-if="loading" class="text-center py-16 text-slate-400 text-sm">Loading...</div>
        <template v-else>
          <ArticleTable
            ref="articleTableRef"
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

      <!-- Bulk Action Bar: lives INSIDE the article content div so it centers
           within the article table column. Uses sticky positioning to float
           at the bottom of the scrollable area. When the detail panel narrows
           the column, the bar re-centers automatically and wraps if needed. -->
      <BulkActionBar
        :selected-count="selectedCount"
        :llm-ready="llmReady"
        @bulk-include="handleBulkInclude"
        @bulk-reject="handleBulkReject"
        @bulk-move-to-working="handleBulkMoveToWorking"
        @bulk-add-tag="openBulkTagDialog"
        @bulk-add-label="openBulkLabelDialog"
        @bulk-add-to-chat="handleBulkAddToChat"
        @bulk-export="handleBulkExport"
        @bulk-ai-summary="handleBulkAiSummary"
        @clear-selection="clearSelection"
      />
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
      :article-total="resultCount"
      :decision-message="decisionMessage"
      :decision-type="decisionType"
      :open-reader-id="pendingOpenReaderId"
      @reader-opened="pendingOpenReaderId = null"
      @close="handleCloseDetail"
      @navigate-prev="navigatePrev"
      @navigate-next="navigateNext"
      @screen-article="screenArticle"
      @move-article="handleMoveArticle"
      @update-notes="updateNotes"
      @update-tags="updateTags"
      @update-labels="updateLabels"
      @update-criteria="updateCriteria"
      @update-metadata="updateMetadata"
      @navigate-to-article="navigateToArticle"
      @toggle-full-screen="toggleDetailFullScreen"
      @attach-full-text="handleAttachFullText"
      @delete-article="handleDeleteArticle"
      @clear-ai-reasoning="handleClearAiReasoning"
      @delete-full-text="handleDeleteFullText"
      @read-full-text="handleReadFullText"
      @refresh-article="refreshArticle"
      @article-promoted="handleArticlePromoted"
      @references-updated="handleReferencesUpdated"
    />

    <!-- Bulk Tag Dialog -->
    <Teleport to="body">
      <div
        v-if="bulkTagDialogOpen"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        @click.self="bulkTagDialogOpen = false"
      >
        <div class="bg-white rounded-xl shadow-xl p-6 w-96 max-w-full">
          <h3 class="text-lg font-semibold mb-4">Change Tag of {{ selectedCount }} Articles</h3>
          <SuggestInput
            v-model="bulkInputValue"
            :suggestions="allTags"
            :clear-on-select="false"
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
              class="px-4 py-2 text-sm rounded-lg bg-red-600 text-white hover:bg-red-700 disabled:opacity-40"
              :disabled="!bulkInputValue.trim()"
              title="Remove this tag from all selected articles"
              @click="handleBulkRemoveTag"
            >
              Remove Tag
            </button>
            <button
              class="px-4 py-2 text-sm rounded-lg bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-40"
              :disabled="!bulkInputValue.trim()"
              title="Add this tag to all selected articles"
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
          <h3 class="text-lg font-semibold mb-4">Change Label of {{ selectedCount }} Articles</h3>
          <SuggestInput
            v-model="bulkInputValue"
            :suggestions="allLabels"
            :clear-on-select="false"
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
              class="px-4 py-2 text-sm rounded-lg bg-red-600 text-white hover:bg-red-700 disabled:opacity-40"
              :disabled="!bulkInputValue.trim()"
              title="Remove this label from all selected articles"
              @click="handleBulkRemoveLabel"
            >
              Remove Label
            </button>
            <button
              class="px-4 py-2 text-sm rounded-lg bg-purple-600 text-white hover:bg-purple-700 disabled:opacity-40"
              :disabled="!bulkInputValue.trim()"
              title="Add this label to all selected articles"
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
