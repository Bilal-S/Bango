<template>
  <aside
    class="flex flex-col h-full bg-white border-l border-slate-200 overflow-hidden"
    data-testid="cocitation-detail-panel"
  >
    <div class="flex items-center justify-between px-4 py-3 border-b border-slate-200 gap-2">
      <div class="flex items-center gap-2 min-w-0">
        <h3 class="text-sm font-semibold text-slate-800 truncate">
          {{ paper ? getPublicationTypeLabel(paper.referenceType) : 'Details' }}
        </h3>
        <span
          v-if="paper"
          class="px-2 py-0.5 rounded-full text-[10px] font-semibold uppercase tracking-tight shrink-0"
          :class="statusBadge.classes"
        >
          {{ statusBadge.text }}
        </span>
      </div>
      <div class="flex items-center gap-0.5 shrink-0">
        <button
          v-if="paper && paper.matchedArticleId"
          data-testid="open-linked-record-btn"
          class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-indigo-600 hover:bg-indigo-50 cursor-pointer transition-colors"
          title="Open linked record"
          @click="$emit('open-linked-record', paper.matchedArticleId)"
        >
          <span class="material-symbols-outlined text-base">open_in_new</span>
        </button>
        <button
          class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-slate-600 hover:bg-slate-100 cursor-pointer transition-colors"
          title="Close"
          @click="$emit('close')"
        >
          <span class="material-symbols-outlined text-base">close</span>
        </button>
      </div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto px-4 py-3">
      <!-- Empty state -->
      <div v-if="!paper" class="text-center text-slate-400 py-8">
        <span class="material-symbols-outlined text-3xl mb-2 block">hub</span>
        <p class="text-xs">Select a node to view details.</p>
      </div>

      <!-- Paper info -->
      <template v-else>
        <p class="text-sm font-semibold text-slate-800 leading-snug">{{ paper.title }}</p>
        <p v-if="paper.authors" class="text-xs text-slate-500 mt-1">{{ paper.authors }}</p>
        <div class="flex items-center gap-3 mt-2 text-[11px] text-slate-400">
          <span v-if="paper.year">{{ paper.year }}</span>
          <span v-if="paper.journal" class="italic truncate">{{ paper.journal }}</span>
        </div>

        <!-- DOI link -->
        <a
          v-if="paper.doi"
          :href="`https://doi.org/${paper.doi}`"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex items-center gap-1 mt-2 text-[11px] text-indigo-600 hover:text-indigo-800"
        >
          <span class="material-symbols-outlined text-xs">link</span>
          {{ paper.doi }}
        </a>

        <!-- Stats row -->
        <div class="flex gap-3 mt-3">
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ paper.coCitationCount }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">Cited by (in-scope)</p>
          </div>
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ paper.citationCount }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">Total Citations</p>
          </div>
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ coCitedPapers.length }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">Co-Cited With</p>
          </div>
        </div>

        <!-- Abstract -->
        <div v-if="paper.abstract" class="mt-4">
          <p class="text-[10px] text-slate-400 uppercase tracking-wide mb-1 font-semibold">
            Abstract
          </p>
          <p class="text-xs text-slate-600 leading-relaxed">{{ paper.abstract }}</p>
        </div>

        <!-- Top co-cited partners -->
        <div v-if="coCitedPapers.length > 0" class="mt-5">
          <p class="text-[10px] text-slate-400 uppercase tracking-wide mb-2 font-semibold">
            Top Co-Cited Partners ({{ coCitedPapers.length }})
          </p>
          <ul class="space-y-1.5">
            <li
              v-for="p in coCitedPapers.slice(0, 10)"
              :key="p.id"
              class="text-xs text-slate-600 cursor-pointer hover:text-indigo-600 transition-colors leading-snug"
              @click="$emit('navigate-paper', p.id)"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="flex-1 truncate">{{ p.label }}</span>
                <span class="shrink-0 text-[10px] text-slate-400 font-medium">
                  {{ p.weight.toFixed(3) }}
                </span>
              </div>
              <!-- Strength bar -->
              <div class="mt-0.5 h-1 bg-slate-100 rounded-full overflow-hidden">
                <div
                  class="h-full bg-indigo-400 rounded-full"
                  :style="{ width: `${(p.weight / maxCoCiteWeight) * 100}%` }"
                />
              </div>
            </li>
          </ul>
        </div>
      </template>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { CocitationNode } from '../types/biblio-cocitation';
import { getPublicationTypeLabel } from '@/utils/formatters';

const props = defineProps<{
  paper: CocitationNode | null;
  coCitedPapers: Array<{ id: string; label: string; weight: number }>;
}>();

defineEmits<{
  (e: 'close'): void;
  (e: 'navigate-paper', nodeId: string): void;
  (e: 'open-linked-record', articleId: string): void;
}>();

/** Max co-citation weight for the strength bar scaling. */
const maxCoCiteWeight = computed(() => {
  const weights = props.coCitedPapers.map((p) => p.weight);
  return Math.max(...weights, 0.001);
});

/**
 * Status-aware badge for the matched library article. Renders the article's
 * status alongside "In Library" so users can see at a glance whether a matched
 * paper is included, rejected, working, or a duplicate. Papers without a match
 * show "Reference Only".
 */
const statusBadge = computed<{ text: string; classes: string }>(() => {
  const status = props.paper?.matchedArticleStatus;
  if (!props.paper?.matchedArticleId) {
    return { text: 'Reference Only', classes: 'bg-slate-100 text-slate-600' };
  }
  // Map the article status to a badge label + color.
  switch (status) {
    case 'included':
      return { text: 'In Library:Included', classes: 'bg-emerald-100 text-emerald-800' };
    case 'rejected':
      return { text: 'In Library:Rejected', classes: 'bg-rose-100 text-rose-800' };
    case 'working':
      return { text: 'In Library:Working', classes: 'bg-amber-100 text-amber-800' };
    case 'duplicate':
      return { text: 'In Library:Duplicate', classes: 'bg-slate-100 text-slate-600' };
    default:
      // matchedArticleId set but status missing (shouldn't happen) - fallback.
      return { text: 'In Library', classes: 'bg-emerald-100 text-emerald-800' };
  }
});
</script>
