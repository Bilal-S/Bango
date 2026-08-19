<template>
  <div
    class="flex flex-col gap-4 p-4 bg-white/80 backdrop-blur-sm rounded-xl border border-slate-200/80 shadow-sm"
  >
    <!-- Search with autocomplete -->
    <NetworkSearchBox
      v-model="searchQuery"
      placeholder="Search keywords…"
      :suggestions="suggestions"
      @input="onSearchInput"
      @select-first="onSelectFirst"
      @select="selectSuggestion"
    />

    <!-- Keyword Source Selector -->
    <div>
      <label class="text-xs text-slate-600 mb-1.5 block font-medium">Keyword Sources</label>
      <div class="grid grid-cols-2 gap-1.5">
        <button
          v-for="src in availableSources"
          :key="src.value"
          class="flex items-center justify-between px-2.5 py-1.5 rounded-lg border text-xs cursor-pointer transition-all font-medium"
          :class="
            localSources.includes(src.value)
              ? 'border-indigo-500 bg-indigo-50/50 text-indigo-700'
              : 'border-slate-200 bg-white text-slate-600 hover:bg-slate-50'
          "
          @click="toggleSource(src.value)"
        >
          <span class="truncate" :title="src.label + ' - ' + src.description">{{ src.label }}</span>
          <span
            class="material-symbols-outlined text-[14px]"
            :class="localSources.includes(src.value) ? 'text-indigo-600' : 'text-slate-300'"
          >
            {{ localSources.includes(src.value) ? 'check_box' : 'check_box_outline_blank' }}
          </span>
        </button>
      </div>
    </div>

    <!-- Min occurrences slider -->
    <NetworkThresholdSlider
      v-model="localMinOccurrences"
      label="Min. Document Frequency"
      :min="1"
      :max="10"
      :step="1"
      @commit="onParamsChange"
    />

    <!-- Min co-occurrence slider -->
    <NetworkThresholdSlider
      v-model="localMinCooccurrence"
      label="Min. Co-occurrence Strength"
      :min="1"
      :max="10"
      :step="1"
      @commit="onParamsChange"
    />

    <!-- Color Mode toggle -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block font-medium">Color Mode</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          :class="
            colorMode === 'cluster'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1.5 text-xs font-medium transition-colors cursor-pointer"
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
          class="flex-1 px-3 py-1.5 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('color-mode-change', 'temporal')"
        >
          Temporal
        </button>
      </div>
    </div>

    <!-- Layout Mode toggle -->
    <div>
      <label class="text-xs text-slate-600 mb-1 block font-medium">Layout</label>
      <div class="flex rounded-lg overflow-hidden border border-slate-200">
        <button
          :class="
            layoutMode === 'fixed'
              ? 'bg-indigo-600 text-white'
              : 'bg-white text-slate-600 hover:bg-slate-50'
          "
          class="flex-1 px-3 py-1.5 text-xs font-medium transition-colors cursor-pointer"
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
          class="flex-1 px-3 py-1.5 text-xs font-medium transition-colors cursor-pointer"
          @click="$emit('layout-mode-change', 'dynamic')"
        >
          Dynamic
        </button>
      </div>
    </div>

    <!-- Stats row -->
    <div class="flex items-center gap-4 text-xs text-slate-500 border-t border-slate-100 pt-3">
      <div class="flex items-center gap-1">
        <span class="material-symbols-outlined text-sm">tag</span>
        <span>{{ visibleNodes }} / {{ totalNodes }} terms</span>
      </div>
      <div class="flex items-center gap-1">
        <span class="material-symbols-outlined text-sm">share</span>
        <span>{{ visibleEdges }} / {{ totalEdges }} co-occurrences</span>
      </div>
    </div>

    <!-- Legends -->
    <div class="border-t border-slate-100 pt-3">
      <!-- Cluster legend -->
      <div v-if="colorMode === 'cluster' && clusters.length > 0">
        <div class="flex items-center justify-between mb-2">
          <p class="text-xs text-slate-500">Clusters (Louvain Communities)</p>
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
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium cursor-pointer transition-all font-semibold"
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
        <p class="text-xs text-slate-500 mb-2 font-medium">Average Publication Year</p>
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
        class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        @click="$emit('fit-screen')"
      >
        <span class="material-symbols-outlined text-sm">fit_screen</span>
        Fit
      </button>
      <button
        class="w-8 h-8 flex items-center justify-center text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        title="Reset Filters"
        @click="resetAnalysis"
      >
        <span class="material-symbols-outlined text-base">restart_alt</span>
      </button>
      <NetworkExportMenu @select="onExport" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { CLUSTER_PALETTE } from '../types/biblio-network';
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
  keywordLabels: string[];
  colorMode: 'cluster' | 'temporal';
  minYear: number;
  maxYear: number;
  selectedClusters: number[];
  sources: string[];
  minOccurrences: number;
  minCooccurrence: number;
  layoutMode: 'fixed' | 'dynamic';
}>();

const emit = defineEmits<{
  (
    e: 'filter-change',
    filters: { minOccurrences: number; minCooccurrence: number; search: string }
  ): void;
  (e: 'locate-keyword', label: string): void;
  (e: 'export-image', format: 'png' | 'gexf'): void;
  (e: 'color-mode-change', mode: 'cluster' | 'temporal'): void;
  (e: 'layout-mode-change', mode: 'fixed' | 'dynamic'): void;
  (e: 'select-cluster', clusterId: number): void;
  (e: 'clear-clusters'): void;
  (
    e: 'params-change',
    params: { sources: string[]; minOccurrences: number; minCooccurrence: number }
  ): void;
  (e: 'fit-screen'): void;
  (e: 'reset-analysis'): void;
}>();

const availableSources = [
  { value: 'metadata', label: 'Metadata', description: 'Keywords defined in article metadata' },
  {
    value: 'ai_extracted',
    label: 'AI Noun Phrases',
    description: 'Noun phrases extracted from abstracts using LLM',
  },
  { value: 'tags', label: 'Tags', description: 'User-defined tags assigned to articles' },
  {
    value: 'labels',
    label: 'Labels',
    description: 'Screening criteria labels matched to articles',
  },
  {
    value: 'user_added',
    label: 'User Added',
    description: 'Custom keywords added manually by the user',
  },
];

const searchQuery = ref('');
const localSources = ref<string[]>([...props.sources]);
const localMinOccurrences = ref(props.minOccurrences);
const localMinCooccurrence = ref(props.minCooccurrence);

watch(
  () => props.sources,
  (val) => {
    localSources.value = [...val];
  },
  { deep: true }
);

watch(
  () => props.minOccurrences,
  (val) => {
    localMinOccurrences.value = val;
  }
);

watch(
  () => props.minCooccurrence,
  (val) => {
    localMinCooccurrence.value = val;
  }
);

const suggestions = computed<NetworkSearchSuggestion[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q || q.length < 2) return [];
  return props.keywordLabels
    .filter((label) => label.toLowerCase().includes(q))
    .slice(0, 8)
    .map((label) => ({ key: label, display: label, payload: label }));
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
  emitFilterChange();
}

/** Enter-select: locate the keyword without re-emitting the filter payload
 *  (preserves the historical commit-on-release search semantics). */
function onSelectFirst(s: NetworkSearchSuggestion) {
  emit('locate-keyword', s.payload);
}

function selectSuggestion(s: NetworkSearchSuggestion) {
  emit('locate-keyword', s.payload);
  emitFilterChange();
}

function toggleSource(val: string) {
  const idx = localSources.value.indexOf(val);
  if (idx > -1) {
    // Keep at least one source active
    if (localSources.value.length > 1) {
      localSources.value.splice(idx, 1);
    }
  } else {
    localSources.value.push(val);
  }
  onParamsChange();
}

function onParamsChange() {
  emit('params-change', {
    sources: localSources.value,
    minOccurrences: localMinOccurrences.value,
    minCooccurrence: localMinCooccurrence.value,
  });
}

function emitFilterChange() {
  emit('filter-change', {
    minOccurrences: localMinOccurrences.value,
    minCooccurrence: localMinCooccurrence.value,
    search: searchQuery.value,
  });
}

function onExport(format: 'png' | 'gexf') {
  emit('export-image', format);
}

function resetAnalysis() {
  searchQuery.value = '';
  localSources.value = ['metadata', 'ai_extracted', 'tags', 'labels', 'user_added'];
  localMinOccurrences.value = 2;
  localMinCooccurrence.value = 2;
  onParamsChange();
  emitFilterChange();
  emit('reset-analysis');
}
</script>
