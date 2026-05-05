<script setup lang="ts">
import type { ArticleQuery } from '@/composables/use-article-search';

defineProps<{ query: ArticleQuery; articleCount: number }>();

const emit = defineEmits<{
  search: [];
  update: [key: string, value: unknown];
}>();
</script>

<template>
  <div
    class="flex items-center justify-between mb-6 bg-white p-3 rounded-xl border border-slate-200 shadow-sm"
  >
    <div class="flex items-center gap-3">
      <!-- Search -->
      <div class="relative">
        <span
          class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-slate-400 text-[18px]"
        >
          search
        </span>
        <input
          type="text"
          placeholder="Search articles..."
          class="pl-9 pr-4 py-1.5 bg-slate-50 border border-slate-200 rounded-lg text-sm w-64 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
          :value="query.search ?? ''"
          @input="emit('update', 'search', ($event.target as HTMLInputElement).value || null)"
          @keyup.enter="emit('search')"
        />
      </div>

      <!-- Status Filter -->
      <div
        class="flex items-center gap-2 px-3 py-1.5 bg-slate-100 rounded-lg text-slate-700 text-sm font-medium cursor-pointer hover:bg-slate-200 transition-colors"
      >
        <span class="material-symbols-outlined text-[18px]">filter_list</span>
        <select
          class="bg-transparent outline-none cursor-pointer text-sm font-medium"
          :value="query.status ?? ''"
          @change="emit('update', 'status', ($event.target as HTMLSelectElement).value || null)"
        >
          <option value="">All Status</option>
          <option value="imported">Imported</option>
          <option value="working">Working</option>
          <option value="included">Included</option>
          <option value="rejected">Rejected</option>
        </select>
      </div>

      <!-- Sort -->
      <div
        class="flex items-center gap-2 px-3 py-1.5 bg-white border border-slate-200 rounded-lg text-slate-600 text-sm cursor-pointer hover:bg-slate-50 transition-colors"
      >
        <span class="material-symbols-outlined text-[18px]">sort</span>
        <select
          class="bg-transparent outline-none cursor-pointer text-sm"
          :value="query.sortBy ?? 'imported_at'"
          @change="emit('update', 'sortBy', ($event.target as HTMLSelectElement).value)"
        >
          <option value="imported_at">Date Added</option>
          <option value="title">Title</option>
          <option value="publicationYear">Year</option>
          <option value="aiConfidence">Confidence</option>
        </select>
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
