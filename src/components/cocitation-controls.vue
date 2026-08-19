<template>
  <div
    class="flex flex-col gap-4 p-4 bg-white/80 backdrop-blur-sm rounded-xl border border-slate-200/80 shadow-sm"
  >
    <!-- Search with autocomplete -->
    <NetworkSearchBox
      v-model="searchQuery"
      placeholder="Search papers…"
      :suggestions="suggestions"
      clearable
      @input="onSearchInput"
      @select-first="onSuggestionChosen"
      @select="onSuggestionChosen"
      @clear="clearSearch"
    />

    <!-- Scope toggle -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block">Scope</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          :class="
            scope === 'included'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('scope-change', 'included')"
        >
          Included
        </button>
        <button
          :class="
            scope === 'all'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('scope-change', 'all')"
        >
          All Articles
        </button>
      </div>
    </div>

    <!-- Normalization mode -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block">Normalization</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          v-for="mode in normalizationModes"
          :key="mode.value"
          :class="
            normalization === mode.value
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-2 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('normalization-change', mode.value)"
        >
          {{ mode.label }}
        </button>
      </div>
    </div>

    <!-- Min citation count -->
    <NetworkThresholdSlider
      :model-value="minCitationCount"
      label="Min. Citation Count"
      :min="1"
      :max="20"
      @input="(v) => emit('min-citation-change', v)"
    />

    <!-- Min co-citation -->
    <NetworkThresholdSlider
      :model-value="minCoCitation"
      label="Min. Co-Citation"
      :min="1"
      :max="20"
      @input="(v) => emit('min-co-citation-change', v)"
    />

    <!-- Hide rejected matches toggle -->
    <label class="flex items-center gap-2 text-xs text-slate-600 cursor-pointer">
      <input
        type="checkbox"
        :checked="hideRejectedMatches"
        class="accent-indigo-600"
        @change="$emit('update:hideRejectedMatches', ($event.target as HTMLInputElement).checked)"
      />
      <span>Hide rejected-article matches</span>
    </label>

    <!-- Recalculate Layout (grouped with threshold controls that trigger re-layout) -->
    <button
      class="flex items-center justify-center gap-1.5 w-full px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
      @click="$emit('recalculate')"
    >
      <span class="material-symbols-outlined text-sm">tune</span>
      Recalculate Layout
    </button>

    <!-- Color Mode toggle -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block">Color Mode</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          :class="
            colorMode === 'cluster'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('color-mode-change', 'cluster')"
        >
          Cluster
        </button>
        <button
          :class="
            colorMode === 'temporal'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('color-mode-change', 'temporal')"
        >
          Temporal
        </button>
      </div>
    </div>

    <!-- Layout Mode toggle -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block">Layout</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          :class="
            layoutMode === 'fixed'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('layout-mode-change', 'fixed')"
        >
          Fixed
        </button>
        <button
          :class="
            layoutMode === 'dynamic'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('layout-mode-change', 'dynamic')"
        >
          Dynamic
        </button>
      </div>
    </div>

    <!-- Stats row -->
    <div class="flex items-center gap-4 text-xs text-slate-500 border-t border-slate-100 pt-3">
      <div class="flex items-center gap-1">
        <span class="material-symbols-outlined text-sm">description</span>
        <span>{{ visibleNodes }} / {{ totalNodes }} papers</span>
      </div>
      <div class="flex items-center gap-1">
        <span class="material-symbols-outlined text-sm">hub</span>
        <span>{{ totalEdges }} links</span>
      </div>
    </div>

    <!-- Legends -->
    <div class="border-t border-slate-100 pt-3">
      <!-- Cluster legend -->
      <div v-if="colorMode === 'cluster' && clusters.length > 0">
        <div class="flex items-center justify-between mb-2">
          <p class="text-xs text-slate-500">Clusters</p>
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

      <!-- Temporal legend -->
      <div v-else-if="colorMode === 'temporal'">
        <p class="text-xs text-slate-500 mb-2">Publication Year</p>
        <div class="flex flex-col gap-1.5">
          <div class="h-3 w-full rounded bg-gradient-to-r from-[#56B4E9] to-[#E69F00]"></div>
          <div class="flex justify-between text-[10px] text-slate-400 font-medium px-0.5">
            <span>{{ minYear }}</span>
            <span>{{ Math.round((minYear + maxYear) / 2) }}</span>
            <span>{{ maxYear }}</span>
          </div>
          <div class="flex items-center gap-1.5 mt-1">
            <span class="h-2.5 w-2.5 rounded-full bg-slate-200 border border-slate-300"></span>
            <span class="text-[10px] text-slate-400 italic">No year data</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Actions -->
    <div class="flex items-center gap-2 border-t border-slate-100 pt-3">
      <button
        class="w-8 h-8 flex items-center justify-center text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        title="Reset Analysis"
        @click="$emit('reset-analysis')"
      >
        <span class="material-symbols-outlined text-base">restart_alt</span>
      </button>
      <NetworkExportMenu @select="onExport" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { CLUSTER_PALETTE } from '../types/biblio-network';
import type { NetworkExportFormat } from '../utils/network-export';
import type { NetworkSearchSuggestion } from '../types/network-graph';
import NetworkSearchBox from './network-search-box.vue';
import NetworkThresholdSlider from './network-threshold-slider.vue';
import NetworkExportMenu from './network-export-menu.vue';

const props = defineProps<{
  totalNodes: number;
  totalEdges: number;
  visibleNodes: number;
  clusterCount: number;
  scope: 'included' | 'all';
  normalization: string;
  minCitationCount: number;
  minCoCitation: number;
  /** When true, hide nodes whose matched article has status 'rejected'. */
  hideRejectedMatches: boolean;
  colorMode: 'cluster' | 'temporal';
  layoutMode: 'fixed' | 'dynamic';
  paperLabels: { label: string; display: string; searchText: string }[];
  minYear: number;
  maxYear: number;
  selectedClusters: number[];
}>();

const emit = defineEmits<{
  (e: 'scope-change', scope: 'included' | 'all'): void;
  (e: 'normalization-change', mode: string): void;
  (e: 'min-citation-change', val: number): void;
  (e: 'min-co-citation-change', val: number): void;
  (e: 'update:hideRejectedMatches', value: boolean): void;
  (e: 'color-mode-change', mode: 'cluster' | 'temporal'): void;
  (e: 'layout-mode-change', mode: 'fixed' | 'dynamic'): void;
  (e: 'locate-paper', label: string): void;
  (e: 'filter-change', filters: { search: string }): void;
  (e: 'select-cluster', clusterId: number): void;
  (e: 'clear-clusters'): void;
  (e: 'recalculate'): void;
  (e: 'export-image', format: NetworkExportFormat): void;
  (e: 'reset-analysis'): void;
}>();

const normalizationModes = [
  { value: 'cosine', label: 'Cosine' },
  { value: 'raw', label: 'Raw' },
  { value: 'jaccard', label: 'Jaccard' },
  { value: 'pearson', label: 'Pearson' },
];

const searchQuery = ref('');

const suggestions = computed<NetworkSearchSuggestion[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q || q.length < 2) return [];
  return props.paperLabels
    .filter((p) => p.searchText.includes(q))
    .slice(0, 8)
    .map((p) => ({ key: p.label, display: p.display, payload: p.label }));
});

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

function onSearchInput() {
  emit('filter-change', { search: searchQuery.value });
}

/** Locate the chosen paper and clear the live-hide filter so focus dimming
 *  from locate-paper takes over. Without this the composite `display` string
 *  hides every node. */
function onSuggestionChosen(s: NetworkSearchSuggestion) {
  emit('locate-paper', s.payload);
  clearSearchFilter();
}

/** Clear-button path: the search box already emptied the query; restore all
 *  nodes (no live-hide). */
function clearSearch() {
  clearSearchFilter();
}

/**
 * Emit a filter-change with an empty search string, which restores all nodes.
 * Used after selecting a paper so the graph returns to full visibility (with
 * focus dimming applied by the parent via locate-paper).
 */
function clearSearchFilter() {
  emit('filter-change', { search: '' });
}

function onExport(format: NetworkExportFormat) {
  emit('export-image', format);
}
</script>
