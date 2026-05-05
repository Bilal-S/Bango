<script setup lang="ts">
import type { Article } from '@/types';
import StatusBadge from './status-badge.vue';
import ConfidenceBar from './confidence-bar.vue';

defineProps<{
  articles: Article[];
  selectedId: string | null;
}>();

defineEmits<{
  select: [id: string];
}>();

function formatAuthors(authors: string[]): string {
  if (authors.length === 0) return '---';
  const display = authors.slice(0, 2).join('; ');
  return authors.length > 2 ? `${display} et al.` : display;
}
</script>

<template>
  <div class="bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden">
    <table class="w-full text-left border-collapse">
      <thead class="bg-slate-50/50 border-b border-slate-200">
        <tr>
          <th class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase">Title</th>
          <th class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase">Authors</th>
          <th class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase w-16">Year</th>
          <th class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase">Journal</th>
          <th class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase">Status</th>
          <th class="py-4 px-2 font-display text-label-caps text-slate-500 uppercase w-32">
            Confidence
          </th>
        </tr>
      </thead>
      <tbody class="divide-y divide-slate-100">
        <tr
          v-for="article in articles"
          :key="article.id"
          class="hover:bg-slate-50/80 transition-colors group cursor-pointer"
          :class="{ 'bg-indigo-50/60': selectedId === article.id }"
          @click="$emit('select', article.id)"
        >
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
          <td class="py-5 px-2 text-body-sm text-slate-600 italic">
            {{ article.journal ?? '---' }}
          </td>
          <td class="py-5 px-2">
            <StatusBadge :status="article.status" />
          </td>
          <td class="py-5 px-2">
            <ConfidenceBar :confidence="article.aiConfidence" />
          </td>
        </tr>
      </tbody>
    </table>

    <!-- Empty state -->
    <div v-if="articles.length === 0" class="text-center py-16 text-slate-400 text-sm">
      No articles found. Import an RIS file to get started.
    </div>
  </div>
</template>
