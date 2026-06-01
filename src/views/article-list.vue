<script setup lang="ts">
import { onMounted, ref, computed } from 'vue';
import { useRoute } from 'vue-router';
import { useArticleSearch } from '@/composables/use-article-search';
import type { ArticleFilter } from '@/composables/use-article-search';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';
import ExportDialog from '@/components/export-dialog.vue';

const route = useRoute();

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
  rangeStart,
  rangeEnd,
  pageSize,
  changePageSize,
  executeToolbarSearch,
  clearSearch,
  hasReturnTarget,
  navigateToArticle,
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

const STATUS_TAB_LABELS: Record<string, string> = {
  all: 'All',
  duplicate: 'Duplicates',
  working: 'Working',
  included: 'Included',
  rejected: 'Rejected',
  error: 'Errors',
};

async function handleMoveArticle(id: string, newStatus: string): Promise<void> {
  await moveArticle(id, newStatus);
}

function handleUpdateFilter(key: keyof ArticleFilter, value: unknown): void {
  (filter as Record<string, unknown>)[key] = value;
}
</script>

<template>
  <div class="h-full flex">
    <!-- Main content area -->
    <div class="flex-1 p-container-padding overflow-y-auto">
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
        :total-count="activeTotalCount"
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
          @select="selectArticle"
          @sort="toggleSort"
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
      @close="closeDetail"
      @navigate-prev="navigatePrev"
      @navigate-next="navigateNext"
      @move-article="handleMoveArticle"
      @update-notes="updateNotes"
      @update-tags="updateTags"
      @update-labels="updateLabels"
      @update-criteria="updateCriteria"
      @navigate-to-article="navigateToArticle"
    />
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
