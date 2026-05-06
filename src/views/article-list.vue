<script setup lang="ts">
import { onMounted, computed } from 'vue';
import { useArticleSearch } from '@/composables/use-article-search';
import type { ArticleFilter } from '@/composables/use-article-search';
import ArticleToolbar from '@/components/article-toolbar.vue';
import ArticleTable from '@/components/article-table.vue';
import ArticleDetailPanel from '@/components/article-detail-panel.vue';
import ArticleFilterPanel from '@/components/article-filter-panel.vue';

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
  moveArticle,
  closeDetail,
  setStatusTab,
  toggleSort,
  toggleFilters,
  applyFilters,
  clearFilters,
} = useArticleSearch();

onMounted(search);

const selectedId = computed(() => selectedArticle.value?.id ?? null);

const STATUS_TAB_LABELS: Record<string, string> = {
  all: 'All',
  imported: 'Imported',
  working: 'Working',
  included: 'Included',
  rejected: 'Rejected',
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
    <div
      class="flex-1 p-container-padding overflow-y-auto"
      :class="{ 'opacity-40 pointer-events-none': showDetail }"
    >
      <!-- Header -->
      <div class="mb-6">
        <h2 class="font-h1 text-h1 text-on-surface">Article Repository</h2>
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
        :article-count="articles.length"
        :show-filters="showFilters"
        @toggle-filters="toggleFilters"
        @search="search"
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
        @update:filter="handleUpdateFilter"
      />

      <!-- Article Table -->
      <div v-if="loading" class="text-center py-16 text-slate-400 text-sm">Loading...</div>
      <ArticleTable
        v-else
        :articles="articles"
        :selected-id="selectedId"
        :sort-column="sortColumn"
        :sort-direction="sortDirection"
        @select="selectArticle"
        @sort="toggleSort"
      />
    </div>

    <!-- Detail Panel -->
    <ArticleDetailPanel
      v-if="showDetail && selectedArticle"
      :article="selectedArticle"
      :audit-trail="auditTrail"
      @close="closeDetail"
      @move-article="handleMoveArticle"
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
