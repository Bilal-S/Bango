<script setup lang="ts">
defineProps<{
  searchText: string;
  showFilters: boolean;
  pageSize: number;
  rangeStart: number;
  rangeEnd: number;
  totalCount: number;
  isFiltered: boolean;
  canGoPrev: boolean;
  canGoNext: boolean;
  /** Whether to show the batch reference scrape button (included tab + isPremium) */
  showBatchRefScrape?: boolean;
  /** Whether batch reference scraping is currently running */
  isBatchRefRunning?: boolean;
}>();

const emit = defineEmits<{
  toggleFilters: [];
  'update:searchText': [value: string];
  clearSearch: [];
  search: [];
  exportRis: [];
  changePageSize: [size: number];
  goPrev: [];
  goNext: [];
  clearFilters: [];
  batchScrapeRefs: [];
}>();

const PAGE_SIZES = [10, 25, 50, 100];

function onPageSizeChange(event: Event): void {
  const target = event.target as HTMLSelectElement;
  emit('changePageSize', Number(target.value));
}
</script>

<template>
  <div
    class="toolbar-container flex flex-wrap items-center justify-between mb-6 bg-white p-3 rounded-xl border border-slate-200 shadow-sm gap-3"
    style="container-type: inline-size"
  >
    <div class="toolbar-left-group flex items-center gap-2 sm:gap-3 min-w-0 flex-1">
      <!-- Clear-filter (only visible when a filter is active). Sits left of
           the Filter toggle so the user sees the "filters are on" affordance
           and can clear them in one click without expanding the panel.
           Turns red when filters are engaged but the panel is collapsed
           (isFiltered && !showFilters) as a stronger "filters are silently
           active" cue; stays neutral when the panel is expanded since the
           user can already see the active filters. -->
      <button
        v-if="isFiltered"
        class="toolbar-clear-filters flex items-center justify-center w-7 h-7 rounded-lg border cursor-pointer transition-colors shrink-0"
        :class="
          !showFilters
            ? 'bg-red-500 border-red-500 text-white hover:bg-red-600 hover:border-red-600'
            : 'border-slate-300 text-slate-600 hover:bg-slate-100 hover:text-slate-900'
        "
        :title="showFilters ? 'Clear filters' : 'Filters active (collapsed) - click to clear'"
        aria-label="Clear filters"
        @click="emit('clearFilters')"
      >
        <span class="material-symbols-outlined text-[18px]">filter_alt_off</span>
      </button>
      <!-- Filter toggle -->
      <button
        class="flex items-center gap-1.5 sm:gap-2 px-2 sm:px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer transition-colors shrink-0"
        :class="
          isFiltered
            ? 'bg-indigo-600 text-white hover:bg-indigo-700'
            : showFilters
              ? 'bg-indigo-100 text-indigo-700'
              : 'bg-slate-100 text-slate-700 hover:bg-slate-200'
        "
        @click="emit('toggleFilters')"
      >
        <span class="material-symbols-outlined text-[18px]">filter_list</span>
        <span class="hidden sm:inline">Filter</span>
      </button>

      <!-- Search input -->
      <div class="toolbar-search-group relative flex items-center min-w-0 flex-1">
        <span
          class="material-symbols-outlined text-[16px] text-slate-400 absolute left-2.5 pointer-events-none"
        >
          search
        </span>
        <input
          type="text"
          :value="searchText"
          placeholder="Search title, abstract, or notes..."
          class="toolbar-search w-full pl-8 pr-7 py-1.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-indigo-400"
          @input="emit('update:searchText', ($event.target as HTMLInputElement).value)"
          @keyup.enter="emit('search')"
        />
        <!-- Clear button -->
        <button
          v-if="searchText"
          class="absolute right-2 flex items-center justify-center w-4 h-4 rounded-full bg-slate-300 hover:bg-slate-400 text-white text-[10px] leading-none transition-colors"
          title="Clear search"
          @click="emit('clearSearch')"
        >
          ×
        </button>
      </div>

      <!-- Search button (icon only) -->
      <button
        class="toolbar-search-btn px-2.5 py-1.5 bg-indigo-600 text-white rounded-lg hover:bg-indigo-700 transition-colors active:scale-95 shrink-0"
        title="Search"
        @click="emit('search')"
      >
        <span class="material-symbols-outlined text-[18px]">search</span>
      </button>
    </div>

    <div class="flex items-center gap-1.5 sm:gap-2 shrink-0">
      <!-- Page size dropdown -->
      <select
        :value="pageSize"
        class="toolbar-page-size px-1.5 sm:px-2 py-1.5 text-xs border border-slate-200 rounded-lg bg-white text-slate-700 focus:outline-none focus:ring-2 focus:ring-indigo-400 cursor-pointer"
        @change="onPageSizeChange"
      >
        <option v-for="size in PAGE_SIZES" :key="size" :value="size">{{ size }}</option>
      </select>

      <!-- Page navigation -->
      <button
        class="px-1.5 sm:px-2 py-1 text-xs rounded border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
        :disabled="!canGoPrev"
        @click="emit('goPrev')"
      >
        &laquo;
      </button>
      <span
        class="flex flex-col items-center text-xs text-slate-500 min-w-[4rem] sm:min-w-[5rem] text-center leading-tight"
      >
        <span
          >{{ rangeStart }}-{{ rangeEnd }}<span class="hidden sm:inline"> of </span
          ><span class="sm:hidden">/</span>{{ totalCount }}</span
        >
        <span
          v-if="isFiltered"
          class="flex items-center gap-1 text-[10px] text-indigo-500 font-medium"
        >
          filtered
          <button
            class="flex items-center justify-center w-3.5 h-3.5 rounded-full bg-indigo-200 hover:bg-indigo-400 text-indigo-600 hover:text-white text-[9px] leading-none transition-colors"
            title="Clear filters"
            @click="emit('clearFilters')"
          >
            ×
          </button>
        </span>
      </span>
      <button
        class="px-1.5 sm:px-2 py-1 text-xs rounded border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
        :disabled="!canGoNext"
        @click="emit('goNext')"
      >
        &raquo;
      </button>

      <!-- Batch Reference Scrape -->
      <button
        v-if="showBatchRefScrape"
        class="toolbar-batch-ref flex items-center gap-1 sm:gap-1.5 px-2 sm:px-3 py-1.5 bg-white border border-slate-200 text-slate-700 rounded-lg text-sm font-medium hover:bg-slate-50 transition-colors shrink-0 disabled:opacity-60 disabled:cursor-not-allowed"
        title="Automatically import all missing references."
        :disabled="isBatchRefRunning"
        @click="emit('batchScrapeRefs')"
      >
        <span v-if="isBatchRefRunning" class="material-symbols-outlined text-[16px] animate-spin"
          >progress_activity</span
        >
        <span v-else class="material-symbols-outlined text-[16px]">Table_Convert</span>
        <span class="hidden sm:inline">{{
          isBatchRefRunning ? 'Extracting from Internet…' : 'Extract Refs'
        }}</span>
      </button>

      <!-- Export -->
      <button
        class="toolbar-export flex items-center gap-1 sm:gap-1.5 px-2 sm:px-3 py-1.5 bg-white border border-slate-200 text-slate-700 rounded-lg text-sm font-medium hover:bg-slate-50 transition-colors shrink-0"
        @click="emit('exportRis')"
      >
        <span class="material-symbols-outlined text-[16px]">download</span>
        <span class="hidden sm:inline">Export</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
/* ── Stacked layout: force left group full-width so right wraps below ── */
@container (max-width: 499px) {
  .toolbar-left-group {
    flex-basis: 100%;
  }
}

/* ── Minimal view: hide search, page size, export ── */
@container (max-width: 299px) {
  .toolbar-search-group,
  .toolbar-search-btn,
  .toolbar-page-size,
  .toolbar-export,
  .toolbar-batch-ref {
    display: none;
  }
}
</style>
