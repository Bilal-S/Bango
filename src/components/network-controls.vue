<template>
  <div
    class="flex flex-col gap-4 p-4 bg-white/80 backdrop-blur-sm rounded-xl border border-slate-200/80 shadow-sm"
  >
    <!-- Search with autocomplete -->
    <NetworkSearchBox
      v-model="searchQuery"
      placeholder="Search authors…"
      :suggestions="suggestions"
      @input="onSearchInput"
      @select-first="onSelectFirst"
      @select="selectSuggestion"
    />

    <!-- Min papers slider -->
    <NetworkThresholdSlider
      v-model="minPapers"
      label="Min. Papers"
      :min="1"
      :max="maxPapersLimit"
      :step="1"
      @input="emitFilters"
    />

    <!-- Min link strength slider -->
    <NetworkThresholdSlider
      v-model="minLinkStrength"
      label="Min. Link Strength"
      :min="1"
      :max="maxLinkLimit"
      :step="1"
      @input="emitFilters"
    />

    <!-- Max authors per document slider -->
    <NetworkThresholdSlider
      v-model="maxAuthors"
      label="Max. Authors per Document"
      :min="20"
      :max="200"
      :step="5"
      @input="emitFilters"
    />

    <!-- Counting mode toggle -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block">Counting Mode</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          :class="
            countingMode === 'full'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('counting-mode-change', 'full')"
        >
          Full
        </button>
        <button
          :class="
            countingMode === 'fractional'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('counting-mode-change', 'fractional')"
        >
          Fractional
        </button>
      </div>
    </div>

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
        <span class="material-symbols-outlined text-sm">circle</span>
        <span>{{ visibleNodes }} / {{ totalNodes }} authors</span>
      </div>
      <div class="flex items-center gap-1">
        <span class="material-symbols-outlined text-sm">timeline</span>
        <span>{{ visibleEdges }} links</span>
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
        <p class="text-xs text-slate-500 mb-2">Avg. Publication Year</p>
        <div class="flex flex-col gap-1.5">
          <!-- Gradient bar: Slate blue (#56B4E9) to Vibrant orange (#E69F00) -->
          <div class="h-3 w-full rounded bg-gradient-to-r from-[#56B4E9] to-[#E69F00]"></div>
          <!-- Labels -->
          <div class="flex justify-between text-[10px] text-slate-400 font-medium px-0.5">
            <span>{{ minYear }}</span>
            <span>{{ Math.round((minYear + maxYear) / 2) }}</span>
            <span>{{ maxYear }}</span>
          </div>
          <!-- Neutral gray for no year -->
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
        class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        @click="$emit('recalculate')"
      >
        <span class="material-symbols-outlined text-sm">fit_screen</span>
        Fit
      </button>
      <button
        class="w-8 h-8 flex items-center justify-center text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        title="Reset Analysis"
        @click="resetAnalysis"
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
import type { CountingMode } from '../types/biblio-network';
import type { NetworkSearchSuggestion } from '../types/network-graph';
import NetworkSearchBox from './network-search-box.vue';
import NetworkThresholdSlider from './network-threshold-slider.vue';
import NetworkExportMenu from './network-export-menu.vue';

const props = defineProps<{
  totalNodes: number;
  totalEdges: number;
  visibleNodes: number;
  visibleEdges: number;
  clusterCount: number;
  authorNames: string[];
  authorWeights: Map<string, number>;
  countingMode: CountingMode;
  colorMode: 'cluster' | 'temporal';
  layoutMode: 'fixed' | 'dynamic';
  minYear: number;
  maxYear: number;
  selectedClusters: number[];
}>();

const emit = defineEmits<{
  (
    e: 'filter-change',
    filters: {
      minPapers: number;
      minLinkStrength: number;
      maxAuthors: number;
      search: string;
    }
  ): void;
  (e: 'locate-author', name: string): void;
  (e: 'export-image', format: 'png' | 'gexf'): void;
  (e: 'counting-mode-change', mode: CountingMode): void;
  (e: 'color-mode-change', mode: 'cluster' | 'temporal'): void;
  (e: 'layout-mode-change', mode: 'fixed' | 'dynamic'): void;
  (e: 'select-cluster', clusterId: number): void;
  (e: 'clear-clusters'): void;
  (e: 'recalculate'): void;
  (e: 'reset-analysis'): void;
}>();

const searchQuery = ref('');
const minPapers = ref(1);
const minLinkStrength = ref(1);
const maxAuthors = ref(20);

const suggestions = computed<NetworkSearchSuggestion[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q || q.length < 2) return [];
  return props.authorNames
    .filter((name) => name.toLowerCase().includes(q))
    .slice(0, 8)
    .map((name) => {
      const weight = props.authorWeights.get(name) ?? 0;
      return {
        key: name,
        display: name,
        detail: `${weight} ${weight === 1 ? 'paper' : 'papers'}`,
        payload: name,
      };
    });
});

const maxPapersLimit = computed(() => Math.max(10, Math.ceil(props.totalNodes / 2)));
const maxLinkLimit = computed(() => Math.max(5, Math.ceil(props.totalEdges / 4)));

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
  emitFilters();
}

/** Enter-select: locate the author without re-emitting the filter payload. */
function onSelectFirst(s: NetworkSearchSuggestion) {
  emit('locate-author', s.payload);
}

function selectSuggestion(s: NetworkSearchSuggestion) {
  emit('locate-author', s.payload);
  emitFilters();
}

function emitFilters() {
  emit('filter-change', {
    minPapers: minPapers.value,
    minLinkStrength: minLinkStrength.value,
    maxAuthors: maxAuthors.value,
    search: searchQuery.value,
  });
}

function onExport(format: 'png' | 'gexf') {
  emit('export-image', format);
}

function resetAnalysis() {
  minPapers.value = 1;
  minLinkStrength.value = 1;
  maxAuthors.value = 20;
  searchQuery.value = '';
  emitFilters();
  emit('reset-analysis');
}
</script>
