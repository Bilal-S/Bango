<template>
  <aside
    class="flex flex-col h-full bg-white border-l border-slate-200 overflow-hidden"
    data-testid="citation-detail-panel"
  >
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3 border-b border-slate-200">
      <h3 class="text-sm font-semibold text-slate-800 truncate">Paper Details</h3>
      <button
        class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-slate-600 hover:bg-slate-100 cursor-pointer transition-colors"
        title="Close"
        @click="$emit('close')"
      >
        <span class="material-symbols-outlined text-base">close</span>
      </button>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto px-4 py-3">
      <!-- Loading state -->
      <div v-if="!paper" class="text-center text-slate-400 py-8">
        <span class="material-symbols-outlined text-3xl mb-2 block">description</span>
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

        <!-- Stats row -->
        <div class="flex gap-3 mt-3">
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ paper.numCited }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">Cited by</p>
          </div>
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ paper.numReferences }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">References</p>
          </div>
        </div>

        <!-- Abstract -->
        <div v-if="paper.abstract" class="mt-4">
          <p class="text-[10px] text-slate-400 uppercase tracking-wide mb-1 font-semibold">
            Abstract
          </p>
          <p class="text-xs text-slate-600 leading-relaxed">{{ paper.abstract }}</p>
        </div>

        <!-- Citing papers (in-edges) -->
        <div v-if="citingPapers.length > 0" class="mt-5">
          <p class="text-[10px] text-slate-400 uppercase tracking-wide mb-2 font-semibold">
            Cited by ({{ citingPapers.length }})
          </p>
          <ul class="space-y-1.5">
            <li
              v-for="p in citingPapers"
              :key="p.id"
              class="text-xs text-slate-600 cursor-pointer hover:text-indigo-600 transition-colors leading-snug"
              @click="$emit('navigate-paper', p.id)"
            >
              <span class="material-symbols-outlined text-[10px] align-middle mr-1"
                >arrow_downward</span
              >
              {{ p.label }}
            </li>
          </ul>
        </div>

        <!-- Cited papers (out-edges) -->
        <div v-if="citedPapers.length > 0" class="mt-5">
          <p class="text-[10px] text-slate-400 uppercase tracking-wide mb-2 font-semibold">
            References ({{ citedPapers.length }})
          </p>
          <ul class="space-y-1.5">
            <li
              v-for="p in citedPapers"
              :key="p.id"
              class="text-xs text-slate-600 cursor-pointer hover:text-indigo-600 transition-colors leading-snug"
              @click="$emit('navigate-paper', p.id)"
            >
              <span class="material-symbols-outlined text-[10px] align-middle mr-1"
                >arrow_upward</span
              >
              {{ p.label }}
            </li>
          </ul>
        </div>
      </template>
    </div>
  </aside>
</template>

<script setup lang="ts">
import type { CitationNode } from '../types/biblio-citation';

defineProps<{
  paper: CitationNode | null;
  citingPapers: { id: string; label: string }[];
  citedPapers: { id: string; label: string }[];
}>();

defineEmits<{
  (e: 'close'): void;
  (e: 'navigate-paper', nodeId: string): void;
}>();
</script>
