<template>
  <div v-if="clusterCount > 0">
    <div class="flex items-center justify-between mb-2">
      <p class="text-xs text-slate-500">{{ title }}</p>
      <div class="flex items-center gap-1">
        <!-- Thematic analysis trigger: exactly one selected cluster + LLM ready -->
        <button
          v-if="llmReady && selectedClusters.length === 1"
          class="h-6 inline-flex items-center gap-1 px-2 text-xs font-medium rounded-md border cursor-pointer transition-colors"
          :class="
            analysisLoading
              ? 'border-indigo-200 bg-indigo-50 text-indigo-400 cursor-wait'
              : 'border-indigo-300 bg-indigo-50 text-indigo-700 hover:bg-indigo-100'
          "
          :disabled="analysisLoading"
          title="Ask the LLM what this cluster's members share"
          @click="$emit('analyze-themes')"
        >
          <span
            class="material-symbols-outlined text-sm"
            :class="analysisLoading ? 'animate-spin' : ''"
          >
            {{ analysisLoading ? 'progress_activity' : 'auto_awesome' }}
          </span>
          Analyze
        </button>
        <button
          class="w-6 h-6 flex items-center justify-center rounded-md border transition-colors cursor-pointer"
          :class="
            selectedClusters.length > 0
              ? 'text-indigo-600 border-indigo-300 bg-indigo-50 hover:bg-indigo-100'
              : 'text-slate-300 border-slate-200 bg-slate-50 cursor-default'
          "
          :disabled="selectedClusters.length === 0"
          title="Clear cluster selection"
          @click="$emit('clear-clusters')"
        >
          <span class="material-symbols-outlined text-sm">filter_alt_off</span>
        </button>
      </div>
    </div>
    <div class="flex flex-wrap gap-1.5">
      <button
        v-for="c in clusters"
        :key="c.id"
        class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium cursor-pointer transition-all"
        :style="{
          backgroundColor: selectedClusters.includes(c.id) ? c.color : c.color + '33',
          color: selectedClusters.includes(c.id) ? '#fff' : c.color,
          outline: selectedClusters.includes(c.id) ? `2px solid ${c.color}` : 'none',
        }"
        @click="$emit('select-cluster', c.id)"
      >
        {{ c.label }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { CLUSTER_PALETTE } from '../types/biblio-network';

/**
 * Shared Louvain cluster legend (extracted from the duplicated markup in
 * network-controls.vue / keyword-controls.vue; citation/cocitation controls
 * can adopt it unchanged when their networks gain thematic analysis).
 *
 * Renders nothing when `clusterCount` is 0. The compact "Analyze" button
 * sits in the heading row between the title and the clear-filter icon
 * (matched to its h-6 height): the single-cluster thematic-analysis trigger,
 * visible only when exactly one cluster is selected AND the canonical LLM
 * gate is true. The spinning glyph + disabled state carry the loading
 * feedback, so the label stays stable-width.
 */
const props = withDefaults(
  defineProps<{
    clusterCount: number;
    selectedClusters: number[];
    /** Canonical `useLlmConfigured()` gate value passed down by the view. */
    llmReady?: boolean;
    /** Disables the trigger with a spinner while the analysis is in flight. */
    analysisLoading?: boolean;
    title?: string;
  }>(),
  { llmReady: false, analysisLoading: false, title: 'Clusters' }
);

defineEmits<{
  (e: 'select-cluster', clusterId: number): void;
  (e: 'clear-clusters'): void;
  (e: 'analyze-themes'): void;
}>();

const clusters = computed(() => {
  const result: { id: number; color: string; label: string }[] = [];
  for (let i = 0; i < props.clusterCount; i++) {
    result.push({
      id: i,
      color: CLUSTER_PALETTE[i % CLUSTER_PALETTE.length]!,
      label: `Cluster ${i + 1}`,
    });
  }
  return result;
});
</script>
