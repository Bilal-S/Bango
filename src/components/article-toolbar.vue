<script setup lang="ts">
defineProps<{
  articleCount: number;
  showFilters: boolean;
}>();

const emit = defineEmits<{
  toggleFilters: [];
  search: [];
}>();
</script>

<template>
  <div
    class="flex items-center justify-between mb-6 bg-white p-3 rounded-xl border border-slate-200 shadow-sm"
  >
    <div class="flex items-center gap-3">
      <!-- Filter toggle -->
      <button
        class="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer transition-colors"
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

      <!-- Sort indicator -->
      <div
        class="flex items-center gap-2 px-3 py-1.5 bg-white border border-slate-200 rounded-lg text-slate-600 text-sm cursor-default"
      >
        <span class="material-symbols-outlined text-[18px]">sort</span>
        Sort: Relevance
      </div>
    </div>

    <div class="flex items-center gap-2">
      <span class="text-xs text-slate-400 mr-2">{{ articleCount }} articles</span>
      <button
        class="px-3 py-1.5 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 transition-colors active:scale-95"
        @click="emit('search')"
      >
        Search
      </button>
    </div>
  </div>
</template>
