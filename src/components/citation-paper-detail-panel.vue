<template>
  <aside
    class="flex flex-col h-full bg-white border-l border-slate-200 overflow-hidden"
    data-testid="citation-detail-panel"
  >
    <div class="flex items-center justify-between px-4 py-3 border-b border-slate-200 gap-2">
      <div class="flex items-center gap-2 min-w-0">
        <h3 class="text-sm font-semibold text-slate-800 truncate">Paper Details</h3>
        <span
          v-if="paper"
          class="px-2 py-0.5 rounded-full text-[10px] font-semibold uppercase tracking-tight shrink-0"
          :class="
            paper.unmatched ? 'bg-slate-100 text-slate-600' : 'bg-emerald-100 text-emerald-800'
          "
        >
          {{ paper.unmatched ? 'Reference Only' : 'Included' }}
        </span>
      </div>
      <button
        class="w-7 h-7 flex items-center justify-center rounded-md text-slate-400 hover:text-slate-600 hover:bg-slate-100 cursor-pointer transition-colors shrink-0"
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

        <!-- Phase 3 — Main Path badge -->
        <div
          v-if="onMainPath"
          class="mt-3 inline-flex items-center gap-1 bg-amber-50 border border-amber-200 rounded-full px-2.5 py-0.5"
        >
          <span class="material-symbols-outlined text-[11px] text-amber-600">route</span>
          <span class="text-[11px] text-amber-700 font-medium">On Main Path</span>
        </div>

        <!-- Stats row -->
        <div class="flex gap-3 mt-3">
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ paper.numCited }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">Cited by</p>
            <p
              v-if="paper.numCited > 0 && citingPapers.length === 0"
              class="text-[9px] text-slate-400 lowercase mt-0.5 leading-none"
            >
              (no details available)
            </p>
          </div>
          <div class="flex-1 bg-slate-50 rounded-lg px-2.5 py-1.5 text-center">
            <p class="text-base font-bold text-slate-700">{{ paper.numReferences }}</p>
            <p class="text-[10px] text-slate-400 uppercase tracking-wide">References</p>
            <p
              v-if="paper.numReferences > 0 && citedPapers.length === 0"
              class="text-[9px] text-slate-400 lowercase mt-0.5 leading-none"
            >
              (no details available)
            </p>
          </div>
        </div>

        <!-- Isolation controls -->
        <div class="mt-3 space-y-1.5">
          <!-- Active isolation badge (indicator only; buttons remain visible below) -->
          <div
            v-if="isAncestryActive || isProgenyActive"
            class="flex items-center justify-between bg-indigo-50 border border-indigo-200 rounded-lg px-2.5 py-1.5"
          >
            <span class="text-[11px] text-indigo-700 font-medium flex items-center gap-1">
              <span class="material-symbols-outlined text-xs">{{
                isAncestryActive ? 'arrow_upward' : 'arrow_downward'
              }}</span>
              {{ isAncestryActive ? 'Ancestry' : 'Progeny' }} isolated
            </span>
            <button
              class="text-[11px] text-indigo-600 hover:text-indigo-800 font-semibold cursor-pointer"
              @click="$emit('clear-isolation')"
            >
              Show All
            </button>
          </div>

          <!-- Both buttons always visible. Active direction is highlighted. -->
          <button
            data-testid="isolate-ancestry-btn"
            class="w-full flex items-center justify-center gap-1.5 text-[11px] font-medium rounded-lg px-2.5 py-1.5 cursor-pointer transition-colors border"
            :class="
              isAncestryActive
                ? 'bg-indigo-600 text-white border-indigo-600 hover:bg-indigo-700'
                : 'text-slate-600 bg-slate-50 hover:bg-slate-100 border-slate-200'
            "
            :title="
              isAncestryActive
                ? 'Ancestry isolated — click to return to Show All'
                : 'Dim all nodes except this paper and its references (transitively)'
            "
            @click="onIsolateClick('ancestry')"
          >
            <span class="material-symbols-outlined text-xs">arrow_upward</span>
            Isolate Ancestry
          </button>
          <button
            data-testid="isolate-progeny-btn"
            class="w-full flex items-center justify-center gap-1.5 text-[11px] font-medium rounded-lg px-2.5 py-1.5 cursor-pointer transition-colors border"
            :class="
              isProgenyActive
                ? 'bg-indigo-600 text-white border-indigo-600 hover:bg-indigo-700'
                : 'text-slate-600 bg-slate-50 hover:bg-slate-100 border-slate-200'
            "
            :title="
              isProgenyActive
                ? 'Progeny isolated — click to return to Show All'
                : 'Dim all nodes except this paper and its citing papers (transitively)'
            "
            @click="onIsolateClick('progeny')"
          >
            <span class="material-symbols-outlined text-xs">arrow_downward</span>
            Isolate Progeny
          </button>
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
import { computed } from 'vue';
import type { CitationNode } from '../types/biblio-citation';
import type { IsolationDirection } from './citation-network-graph.vue';

const props = defineProps<{
  paper: CitationNode | null;
  citingPapers: { id: string; label: string }[];
  citedPapers: { id: string; label: string }[];
  /** Current isolation mode (null when not isolating). */
  isolationMode: { nodeId: string; direction: IsolationDirection } | null;
  /** Phase 3 — Main Path (SPC): set of node IDs on the main path backbone. */
  mainPathNodes?: Set<string>;
}>();

/** Whether the currently-selected paper is on the main path backbone. */
const onMainPath = computed(
  () => !!props.paper && !!props.mainPathNodes && props.mainPathNodes.has(props.paper.id)
);

/** Whether ancestry isolation is active for the currently-selected paper. */
const isAncestryActive = computed(
  () =>
    !!props.isolationMode &&
    props.isolationMode.nodeId === props.paper?.id &&
    props.isolationMode.direction === 'ancestry'
);

/** Whether progeny isolation is active for the currently-selected paper. */
const isProgenyActive = computed(
  () =>
    !!props.isolationMode &&
    props.isolationMode.nodeId === props.paper?.id &&
    props.isolationMode.direction === 'progeny'
);

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'navigate-paper', nodeId: string): void;
  (e: 'isolate', direction: IsolationDirection): void;
  (e: 'clear-isolation'): void;
}>();

/**
 * Handle an isolation button click.
 *
 * If the clicked direction is already active, toggle off (emit `clear-isolation`).
 * Otherwise, emit `isolate` with the new direction — the parent replaces the
 * isolation mode, which implicitly clears any previously-active direction.
 */
function onIsolateClick(direction: IsolationDirection) {
  const isActive = direction === 'ancestry' ? isAncestryActive.value : isProgenyActive.value;
  if (isActive) {
    emit('clear-isolation');
  } else {
    emit('isolate', direction);
  }
}
</script>
