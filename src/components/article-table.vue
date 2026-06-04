<script setup lang="ts">
import { watch, nextTick } from 'vue';
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import ConfidenceBar from './confidence-bar.vue';

const props = defineProps<{
  articles: Article[];
  selectedId: string | null;
  sortColumn: string | null;
  sortDirection: 'asc' | 'desc';
  selectedIds: Set<string>;
  allSelected: boolean;
  someSelected: boolean;
}>();

defineEmits<{
  select: [id: string];
  sort: [column: string];
  toggleSelect: [id: string];
  toggleSelectAll: [];
}>();

interface ColumnDef {
  key: string;
  label: string;
  width?: string;
  responsiveClass?: string;
  sortable?: boolean;
}

const COLUMNS: ColumnDef[] = [
  { key: 'index', label: '#', width: 'w-12', responsiveClass: 'col-index', sortable: false },
  { key: 'title', label: 'Title' },
  { key: 'authors', label: 'Authors' },
  { key: 'publicationYear', label: 'Year', width: 'w-16' },
  { key: 'journal', label: 'Journal', responsiveClass: 'col-journal' },
  { key: 'status', label: 'Status' },
  { key: 'aiConfidence', label: 'Confidence', width: 'w-16', responsiveClass: 'col-confidence' },
  { key: 'changedAt', label: 'Changed', width: 'w-28', responsiveClass: 'col-changed' },
];

function formatAuthors(authors: string[]): string {
  if (authors.length === 0) return '---';
  const display = authors.slice(0, 2).join('; ');
  return authors.length > 2 ? `${display} et al.` : display;
}

function getSortIcon(columnKey: string): string {
  if (props.sortColumn !== columnKey) return '';
  return props.sortDirection === 'asc' ? 'arrow_upward' : 'arrow_downward';
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '---';
  const date = new Date(dateStr);
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

// Auto-scroll to keep the selected row visible when navigating via prev/next
watch(
  () => props.selectedId,
  (newId) => {
    if (!newId) return;
    void nextTick(() => {
      const row = document.querySelector<HTMLElement>(`tr[data-article-id="${newId}"]`);
      row?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    });
  }
);
</script>

<template>
  <div
    class="article-table-wrapper bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden"
  >
    <div class="article-table-scroll">
      <table class="w-full text-left border-collapse">
        <thead class="bg-slate-50/50 border-b border-slate-200">
          <tr>
            <!-- Checkbox header -->
            <th class="w-10 py-4 px-2">
              <input
                type="checkbox"
                class="accent-indigo-600 rounded cursor-pointer"
                :checked="allSelected"
                :indeterminate="someSelected"
                @change="$emit('toggleSelectAll')"
              />
            </th>
            <th
              v-for="col in COLUMNS"
              :key="col.key"
              class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase select-none"
              :class="[col.width, col.responsiveClass]"
            >
              <button
                v-if="col.sortable !== false"
                class="flex items-center gap-1 hover:text-slate-700 transition-colors"
                @click="$emit('sort', col.key)"
              >
                <span>{{ col.label }}</span>
                <span v-if="sortColumn === col.key" class="text-indigo-600">
                  <span class="material-symbols-outlined text-[16px]">{{
                    getSortIcon(col.key)
                  }}</span>
                </span>
                <span v-else class="text-slate-300">
                  <span class="material-symbols-outlined text-[16px]">arrow_upward</span>
                </span>
              </button>
              <span v-else>{{ col.label }}</span>
            </th>
          </tr>
        </thead>
        <tbody class="divide-y divide-slate-100">
          <tr
            v-for="article in articles"
            :key="article.id"
            :data-article-id="article.id"
            class="hover:bg-slate-50/80 transition-colors group cursor-pointer"
            :class="{ 'bg-indigo-50': selectedId === article.id }"
            @click="$emit('select', article.id)"
          >
            <!-- Checkbox cell -->
            <td class="py-5 px-2" @click.stop>
              <input
                type="checkbox"
                class="accent-indigo-600 rounded cursor-pointer"
                :checked="selectedIds.has(article.id)"
                @change="$emit('toggleSelect', article.id)"
              />
            </td>
            <td
              class="col-index py-5 px-2 text-body-sm text-slate-500 font-mono border-l-4 transition-colors"
              :class="selectedId === article.id ? 'border-l-indigo-600' : 'border-l-transparent'"
            >
              {{ article.sequenceId }}
            </td>
            <td class="py-5 px-2 max-w-xs">
              <p class="text-body-main font-semibold text-slate-900 truncate">
                {{ article.title }}
              </p>
            </td>
            <td class="py-5 px-2 text-body-sm text-slate-600">
              {{ formatAuthors(article.authors) }}
            </td>
            <td class="py-5 px-2 text-body-sm text-slate-600 font-mono">
              {{ article.publicationYear ?? '---' }}
            </td>
            <td class="col-journal py-5 px-2 text-body-sm text-slate-600 italic">
              {{ article.journal ?? '---' }}
            </td>
            <td class="py-5 px-2">
              <StatusBadge :status="article.status" />
            </td>
            <td class="col-confidence py-5 px-2">
              <ConfidenceBar :confidence="article.aiConfidence" />
            </td>
            <td class="col-changed py-5 px-2 text-body-sm text-slate-500">
              {{ formatDate(article.changedAt) }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Empty state -->
    <div v-if="articles.length === 0" class="text-center py-16 text-slate-400 text-sm">
      No articles found. Import an RIS file to get started.
    </div>
  </div>
</template>

<style scoped>
.article-table-scroll {
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}

/* Hide lower-priority columns on smaller viewports */
@media (max-width: 767px) {
  .col-journal,
  .col-changed,
  .col-confidence {
    display: none;
  }
}

@media (max-width: 1023px) and (min-width: 768px) {
  .col-changed {
    display: none;
  }
}
</style>
