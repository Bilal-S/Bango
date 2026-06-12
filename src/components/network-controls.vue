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
        placeholder="Search authors…"
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
          :key="s.id"
          class="px-3 py-1.5 text-sm cursor-pointer hover:bg-indigo-50 text-slate-700 truncate"
          @mousedown.prevent="selectSuggestion(s)"
        >
          {{ s.label }}
          <span class="text-xs text-slate-400 ml-1">({{ s.weight }} papers)</span>
        </li>
      </ul>
    </div>

    <!-- Min papers slider -->
    <div>
      <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
        <span>Min. Papers</span>
        <span class="font-semibold tabular-nums">{{ minPapers }}</span>
      </label>
      <input
        v-model.number="minPapers"
        type="range"
        :min="1"
        :max="maxPapersLimit"
        step="1"
        class="w-full accent-indigo-600"
        @input="emitFilters"
      />
    </div>

    <!-- Min link strength slider -->
    <div>
      <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
        <span>Min. Link Strength</span>
        <span class="font-semibold tabular-nums">{{ minLinkStrength }}</span>
      </label>
      <input
        v-model.number="minLinkStrength"
        type="range"
        :min="1"
        :max="maxLinkLimit"
        step="1"
        class="w-full accent-indigo-600"
        @input="emitFilters"
      />
    </div>

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
        <p class="text-xs text-slate-500 mb-2">Clusters</p>
        <div class="flex flex-wrap gap-1.5">
          <span
            v-for="c in clusters"
            :key="c.id"
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium text-white"
            :style="{ backgroundColor: c.color }"
          >
            {{ c.label }}
          </span>
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
            <span class="h-2-5 w-2-5 rounded-full bg-slate-200 border border-slate-300"></span>
            <span class="text-[10px] text-slate-400 italic">No year data</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Actions -->
    <div class="flex items-center gap-2 border-t border-slate-100 pt-3">
      <button
        class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        @click="$emit('reset-zoom')"
      >
        <span class="material-symbols-outlined text-sm">fit_screen</span>
        Fit
      </button>
      <button
        class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-slate-100 hover:bg-slate-200 rounded-lg cursor-pointer transition-colors"
        @click="$emit('export-image')"
      >
        <span class="material-symbols-outlined text-sm">download</span>
        Export
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { CLUSTER_PALETTE } from '../types/biblio-network';
import type { CountingMode } from '../types/biblio-network';

const props = defineProps<{
  totalNodes: number;
  totalEdges: number;
  visibleNodes: number;
  visibleEdges: number;
  clusterCount: number;
  authorNames: string[];
  countingMode: CountingMode;
  colorMode: 'cluster' | 'temporal';
  minYear: number;
  maxYear: number;
}>();

const emit = defineEmits<{
  (
    e: 'filter-change',
    filters: { minPapers: number; minLinkStrength: number; search: string }
  ): void;
  (e: 'locate-author', name: string): void;
  (e: 'reset-zoom'): void;
  (e: 'export-image'): void;
  (e: 'counting-mode-change', mode: CountingMode): void;
  (e: 'color-mode-change', mode: 'cluster' | 'temporal'): void;
}>();

const searchQuery = ref('');
const minPapers = ref(1);
const minLinkStrength = ref(1);
const showSuggestions = ref(false);

interface AuthorSuggestion {
  id: string;
  label: string;
  weight: number;
}

const suggestions = computed<AuthorSuggestion[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q || q.length < 2) return [];
  return props.authorNames
    .filter((name) => name.toLowerCase().includes(q))
    .slice(0, 8)
    .map((name) => ({
      id: name,
      label: name,
      weight: 0, // weight not available from name list — future: pass full author objects
    }));
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
  showSuggestions.value = true;
  emitFilters();
}

function selectFirstSuggestion() {
  if (suggestions.value.length > 0) {
    const first = suggestions.value[0]!;
    searchQuery.value = first.label;
    showSuggestions.value = false;
    emit('locate-author', first.label);
  }
}

function selectSuggestion(s: AuthorSuggestion) {
  searchQuery.value = s.label;
  showSuggestions.value = false;
  emit('locate-author', s.label);
  emitFilters();
}

function clearSuggestions() {
  showSuggestions.value = false;
}

function emitFilters() {
  emit('filter-change', {
    minPapers: minPapers.value,
    minLinkStrength: minLinkStrength.value,
    search: searchQuery.value,
  });
}
</script>
