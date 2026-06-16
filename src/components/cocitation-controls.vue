<template>
  <div
    class="flex flex-col gap-4 p-4 bg-white/80 backdrop-blur-sm rounded-xl border border-slate-200/80 shadow-sm"
  >
    <!-- Search with autocomplete -->
    <div class="relative">
      <span
        class="material-symbols-outlined absolute left-2.5 top-1/2 -translate-y-1/2 text-slate-400 text-base z-10"
        >search</span
      >
      <input
        v-model="searchQuery"
        type="text"
        placeholder="Search papers…"
        class="w-full pl-8 pr-3 py-1.5 text-sm bg-slate-50 border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-transparent"
        @input="onSearchInput"
        @keydown.enter="selectFirstSuggestion"
        @keydown.escape="clearSuggestions"
        @focus="showSuggestions = true"
      />
      <!-- Autocomplete dropdown -->
      <ul
        v-if="showSuggestions && suggestions.length > 0"
        class="absolute z-20 left-0 right-0 top-full mt-1 max-h-40 overflow-y-auto bg-white border border-slate-200 rounded-lg shadow-lg"
      >
        <li
          v-for="s in suggestions"
          :key="s"
          class="px-3 py-1.5 text-sm cursor-pointer hover:bg-indigo-50 text-slate-700 truncate"
          @mousedown.prevent="selectSuggestion(s)"
        >
          {{ s }}
        </li>
      </ul>
    </div>

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
    <div>
      <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
        <span>Min. Citation Count</span>
        <span class="font-semibold tabular-nums">{{ minCitationCount }}</span>
      </label>
      <input
        type="range"
        min="1"
        max="20"
        :value="minCitationCount"
        class="w-full accent-indigo-600"
        @input="$emit('min-citation-change', Number(($event.target as HTMLInputElement).value))"
      />
    </div>

    <!-- Min co-citation -->
    <div>
      <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
        <span>Min. Co-Citation</span>
        <span class="font-semibold tabular-nums">{{ minCoCitation }}</span>
      </label>
      <input
        type="range"
        min="1"
        max="20"
        :value="minCoCitation"
        class="w-full accent-indigo-600"
        @input="$emit('min-co-citation-change', Number(($event.target as HTMLInputElement).value))"
      />
    </div>

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
      <div class="relative">
        <button
          class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
          @click="showExportMenu = !showExportMenu"
        >
          <span class="material-symbols-outlined text-sm">download</span>
          Export
          <span class="material-symbols-outlined text-sm">expand_more</span>
        </button>
        <ul
          v-if="showExportMenu"
          class="absolute left-0 bottom-full mb-1 w-36 bg-white border border-slate-200 rounded-lg shadow-lg z-30 overflow-hidden"
        >
          <li
            class="px-3 py-2 text-xs text-slate-700 hover:bg-indigo-50 cursor-pointer flex items-center gap-2"
            @click="onExport('png')"
          >
            <span class="material-symbols-outlined text-sm">image</span>
            PNG Image
          </li>
          <li
            class="px-3 py-2 text-xs text-slate-700 hover:bg-indigo-50 cursor-pointer flex items-center gap-2"
            @click="onExport('gexf')"
          >
            <span class="material-symbols-outlined text-sm">share</span>
            GEXF Network
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { CLUSTER_PALETTE } from '../types/biblio-network';
import type { NetworkExportFormat } from '../utils/network-export';

const props = defineProps<{
  totalNodes: number;
  totalEdges: number;
  visibleNodes: number;
  clusterCount: number;
  scope: 'included' | 'all';
  normalization: string;
  minCitationCount: number;
  minCoCitation: number;
  colorMode: 'cluster' | 'temporal';
  layoutMode: 'fixed' | 'dynamic';
  paperLabels: string[];
  minYear: number;
  maxYear: number;
  selectedClusters: number[];
}>();

const emit = defineEmits<{
  (e: 'scope-change', scope: 'included' | 'all'): void;
  (e: 'normalization-change', mode: string): void;
  (e: 'min-citation-change', val: number): void;
  (e: 'min-co-citation-change', val: number): void;
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
const showSuggestions = ref(false);
const showExportMenu = ref(false);

const suggestions = computed<string[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q || q.length < 2) return [];
  return props.paperLabels.filter((label) => label.toLowerCase().includes(q)).slice(0, 8);
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
  showSuggestions.value = true;
  emit('filter-change', { search: searchQuery.value });
}

function selectFirstSuggestion() {
  if (suggestions.value.length > 0) {
    const first = suggestions.value[0]!;
    searchQuery.value = first;
    showSuggestions.value = false;
    emit('locate-paper', first);
  }
}

function selectSuggestion(s: string) {
  searchQuery.value = s;
  showSuggestions.value = false;
  emit('locate-paper', s);
  emit('filter-change', { search: searchQuery.value });
}

function clearSuggestions() {
  showSuggestions.value = false;
}

function onExport(format: NetworkExportFormat) {
  showExportMenu.value = false;
  emit('export-image', format);
}
</script>
