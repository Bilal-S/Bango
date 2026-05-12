<script setup lang="ts">
defineProps<{
  searchText: string;
  showFilters: boolean;
  pageSize: number;
  rangeStart: number;
  rangeEnd: number;
  totalCount: number;
  canGoPrev: boolean;
  canGoNext: boolean;
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
}>();

const PAGE_SIZES = [25, 50, 100];

function onPageSizeChange(event: Event): void {
  const target = event.target as HTMLSelectElement;
  emit('changePageSize', Number(target.value));
}
</script>

<template>
  <div
    class="flex items-center justify-between mb-6 bg-white p-3 rounded-xl border border-slate-200 shadow-sm gap-3"
  >
    <div class="flex items-center gap-3">
      <!-- Filter toggle -->
      <button
        class="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer transition-colors shrink-0"
        :class="
          showFilters
            ? 'bg-indigo-100 text-indigo-700'
            : 'bg-slate-100 text-slate-700 hover:bg-slate-200'
        "
        @click="emit('toggleFilters')"
      >
        <span class="material-symbols-outlined text-[18px]">filter_list</span>
        Filter
      </button>

      <!-- Search input -->
      <div class="relative flex items-center">
        <span
          class="material-symbols-outlined text-[16px] text-slate-400 absolute left-2.5 pointer-events-none"
        >
          search
        </span>
        <input
          type="text"
          :value="searchText"
          placeholder="Search title or abstract..."
          class="pl-8 pr-7 py-1.5 text-sm border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-indigo-400 w-56"
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

      <!-- Search button -->
      <button
        class="px-3 py-1.5 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 transition-colors active:scale-95 shrink-0"
        @click="emit('search')"
      >
        Search
      </button>
    </div>

    <div class="flex items-center gap-2">
      <!-- Page size dropdown -->
      <select
        :value="pageSize"
        class="px-2 py-1.5 text-xs border border-slate-200 rounded-lg bg-white text-slate-700 focus:outline-none focus:ring-2 focus:ring-indigo-400 cursor-pointer"
        @change="onPageSizeChange"
      >
        <option v-for="size in PAGE_SIZES" :key="size" :value="size">{{ size }}</option>
      </select>

      <!-- Page navigation -->
      <button
        class="px-2 py-1 text-xs rounded border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
        :disabled="!canGoPrev"
        @click="emit('goPrev')"
      >
        &laquo;
      </button>
      <span class="text-xs text-slate-500 min-w-[5rem] text-center">
        {{ rangeStart }}-{{ rangeEnd }} of {{ totalCount }}
      </span>
      <button
        class="px-2 py-1 text-xs rounded border border-slate-300 disabled:opacity-40 hover:bg-slate-50 transition-colors"
        :disabled="!canGoNext"
        @click="emit('goNext')"
      >
        &raquo;
      </button>

      <!-- Export -->
      <button
        class="flex items-center gap-1.5 px-3 py-1.5 bg-white border border-slate-200 text-slate-700 rounded-lg text-sm font-medium hover:bg-slate-50 transition-colors shrink-0"
        @click="emit('exportRis')"
      >
        <span class="material-symbols-outlined text-[16px]">download</span>
        Export
      </button>
    </div>
  </div>
</template>
