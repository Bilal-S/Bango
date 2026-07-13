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
        class="w-full pl-8 pr-8 py-1.5 text-sm bg-slate-50 border border-slate-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-transparent"
        @input="onSearchInput"
        @keydown.enter="selectFirstSuggestion"
        @keydown.escape="clearSuggestions"
        @focus="showSuggestions = true"
      />
      <!-- Clear (x) button -->
      <button
        v-if="searchQuery"
        type="button"
        class="absolute right-2 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600 z-10 cursor-pointer"
        title="Clear search"
        @click="clearSearch"
      >
        <span class="material-symbols-outlined text-base">close</span>
      </button>
      <!-- Autocomplete dropdown -->
      <ul
        v-if="showSuggestions && suggestions.length > 0"
        class="absolute z-20 left-0 right-0 top-full mt-1 max-h-40 overflow-y-auto bg-white border border-slate-200 rounded-lg shadow-lg"
      >
        <li
          v-for="s in suggestions"
          :key="s.label"
          class="px-3 py-1.5 text-sm cursor-pointer hover:bg-indigo-50 text-slate-700 truncate"
          @mousedown.prevent="selectSuggestion(s)"
        >
          {{ s.display }}
        </li>
      </ul>
    </div>

    <!-- Min citations slider -->
    <div>
      <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
        <span>Min. Citations Received</span>
        <span class="font-semibold tabular-nums">{{ minCitations }}</span>
      </label>
      <input
        v-model.number="minCitations"
        type="range"
        :min="0"
        :max="maxCitationsLimit"
        step="1"
        class="w-full accent-indigo-600"
        @input="emitFilters"
      />
    </div>

    <!-- Show isolated toggle -->
    <div class="flex items-center justify-between">
      <label class="text-xs text-slate-600">Show Isolated Papers</label>
      <button
        class="relative w-9 h-5 rounded-full transition-colors cursor-pointer"
        :class="showIsolated ? 'bg-indigo-600' : 'bg-slate-300'"
        :title="showIsolated ? 'Showing papers with no citations' : 'Hiding isolated papers'"
        @click="toggleIsolated"
      >
        <span
          class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform"
          :class="showIsolated ? 'translate-x-4' : ''"
        ></span>
      </button>
    </div>

    <!-- Show Unmatched References toggle -->
    <div class="flex items-center justify-between">
      <label class="text-xs text-slate-600 flex items-center gap-1">
        Show Unmatched References
        <span
          class="material-symbols-outlined text-[14px] text-slate-400 cursor-help"
          title="Include reference papers that are not themselves included articles.  Shown as small dashed grey leaves."
          >help</span
        >
      </label>
      <button
        class="relative w-9 h-5 rounded-full transition-colors cursor-pointer"
        :class="showUnmatched ? 'bg-indigo-600' : 'bg-slate-300'"
        :title="
          showUnmatched ? 'Showing unmatched reference papers' : 'Hiding unmatched reference papers'
        "
        @click="toggleUnmatched"
      >
        <span
          class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform"
          :class="showUnmatched ? 'translate-x-4' : ''"
        ></span>
      </button>
    </div>

    <!-- Main Paths (SPC) toggle -->
    <div class="flex items-center justify-between">
      <label class="text-xs text-slate-600 flex items-center gap-1">
        Main Paths (SPC)
        <span
          class="material-symbols-outlined text-[14px] text-slate-400 cursor-help"
          title="Highlight the main backbones using Search Path Count (SPC).  Dims all nodes and edges not on the main paths."
          >help</span
        >
      </label>
      <button
        class="relative w-9 h-5 rounded-full transition-colors cursor-pointer"
        :class="showMainPath ? 'bg-amber-500' : 'bg-slate-300'"
        :title="
          showMainPath ? 'Main path highlight active - click to turn off' : 'Highlight main path'
        "
        @click="toggleMainPath"
      >
        <span
          class="absolute top-0.5 left-0.5 w-4 h-4 bg-white rounded-full transition-transform"
          :class="showMainPath ? 'translate-x-4' : ''"
        ></span>
      </button>
    </div>

    <!-- Time-Slice year range slider -->
    <div v-if="minYear !== maxYear">
      <label class="flex items-center justify-between text-xs text-slate-600 mb-1">
        <span>Time-Slice</span>
        <span class="font-semibold tabular-nums">{{ yearStart }} – {{ yearEnd }}</span>
      </label>
      <div class="relative h-6 flex items-center">
        <!-- Track -->
        <div class="absolute inset-x-0 h-1.5 bg-slate-200 rounded-full"></div>
        <!-- Active range highlight -->
        <div
          class="absolute h-1.5 bg-indigo-500 rounded-full pointer-events-none"
          :style="{
            left: `${((yearStart - minYear) / (maxYear - minYear)) * 100}%`,
            right: `${((maxYear - yearEnd) / (maxYear - minYear)) * 100}%`,
          }"
        ></div>
        <!-- Left handle -->
        <input
          v-model.number="yearStart"
          type="range"
          :min="minYear"
          :max="maxYear"
          step="1"
          class="dual-range absolute inset-0 w-full"
          @input="onYearInput"
          @change="onYearChange"
        />
        <!-- Right handle -->
        <input
          v-model.number="yearEnd"
          type="range"
          :min="minYear"
          :max="maxYear"
          step="1"
          class="dual-range absolute inset-0 w-full"
          @input="onYearInput"
          @change="onYearChange"
        />
      </div>
      <div class="flex justify-between text-[10px] text-slate-400 mt-0.5">
        <span>{{ minYear }}</span>
        <span>{{ maxYear }}</span>
      </div>
      <button
        v-if="yearActive"
        class="mt-1 text-[10px] text-indigo-600 hover:text-indigo-700 cursor-pointer"
        @click="clearYearSlice"
      >
        Reset year filter
      </button>
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

    <!-- Active Isolation Badge -->
    <div v-if="isolationMode" class="flex flex-col gap-1.5 border-t border-slate-100 pt-3">
      <span class="text-xs font-medium text-slate-500">Active Isolation</span>
      <div
        class="flex items-center justify-between bg-indigo-50 border border-indigo-100 rounded-lg px-2.5 py-1.5 text-xs text-indigo-700"
      >
        <div class="flex items-center gap-1.5 min-w-0">
          <span class="material-symbols-outlined text-sm shrink-0">filter_alt</span>
          <span class="font-medium truncate" :title="isolationLabel">{{ isolationLabel }}</span>
        </div>
        <button
          class="material-symbols-outlined text-sm cursor-pointer text-indigo-400 hover:text-indigo-600 transition-colors ml-1.5 shrink-0 animate-none"
          title="Clear isolation"
          @click="$emit('clear-isolation')"
        >
          close
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
        <span class="material-symbols-outlined text-sm">arrow_right_alt</span>
        <span>{{ visibleEdges }} citations</span>
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
import { ref, computed, watch } from 'vue';
import { CITATION_CLUSTER_PALETTE } from '../types/biblio-citation';

const props = defineProps<{
  totalNodes: number;
  totalEdges: number;
  visibleNodes: number;
  visibleEdges: number;
  clusterCount: number;
  paperLabels: { label: string; display: string; searchText: string }[];
  colorMode: 'cluster' | 'temporal';
  layoutMode: 'fixed' | 'dynamic';
  minYear: number;
  maxYear: number;
  selectedClusters: number[];
  /** Current state of the "Show Unmatched References" toggle (mirrored back). */
  showUnmatched: boolean;
  /** Current state of the "Main Path (SPC)" highlight toggle. */
  showMainPath: boolean;
  /** Current active isolation mode. */
  isolationMode: { nodeId: string; direction: 'ancestry' | 'progeny'; label?: string } | null;
}>();

const emit = defineEmits<{
  (
    e: 'filter-change',
    filters: {
      minCitations: number;
      showIsolated: boolean;
      search: string;
      yearRange?: [number, number] | null;
    }
  ): void;
  (e: 'locate-paper', label: string): void;
  (e: 'export-image', format: 'png' | 'gexf'): void;
  (e: 'color-mode-change', mode: 'cluster' | 'temporal'): void;
  (e: 'layout-mode-change', mode: 'fixed' | 'dynamic'): void;
  (e: 'select-cluster', clusterId: number): void;
  (e: 'clear-clusters'): void;
  (e: 'recalculate'): void;
  (e: 'reset-analysis'): void;
  /**
   * Emitted when the "Show Unmatched References" toggle changes.  The parent
   * re-fetches the network from the backend with the new flag, because adding
   * or removing unmatched leaf nodes cannot be done with client-side filtering
   * alone.
   */
  (e: 'unmatched-change', showUnmatched: boolean): void;
  /**
   * Phase 2 - Time-Slice: emitted on every `input` event (dragging a slider
   * handle).  The parent applies the year-range filter immediately to hide/show
   * nodes but defers the expensive ForceAtlas2 re-layout until `change` (slider
   * release) is emitted via `year-range-commit`.
   */
  (
    e: 'year-range-input',
    range: [number, number],
    filters?: { minCitations: number; showIsolated: boolean; search: string }
  ): void;
  /**
   * Phase 2 - Time-Slice: emitted on `change` (slider release).  The parent runs
   * the ForceAtlas2 re-layout on the now-filtered subgraph.
   */
  (e: 'year-range-commit', range: [number, number]): void;
  /**
   * Phase 3 - Main Path: emitted when the user toggles the SPC highlight.  The
   * parent triggers the worker computation and applies the visual highlight.
   */
  (e: 'main-path-change', showMainPath: boolean): void;
  /** Emitted when the user clears the isolation mode from the sidebar badge. */
  (e: 'clear-isolation'): void;
}>();

const searchQuery = ref('');
const minCitations = ref(0);
const showIsolated = ref(true);
/**
 * Local toggle state for "Show Unmatched References".  Kept in sync with the
 * `showUnmatched` prop so the parent remains the source of truth (it owns the
 * refetch), while still allowing two-way toggle behaviour.
 */
const showUnmatched = ref(props.showUnmatched);
watch(
  () => props.showUnmatched,
  (v) => {
    showUnmatched.value = v;
  }
);
const showSuggestions = ref(false);
const showExportMenu = ref(false);

/**
 * Phase 2 - Time-Slice year-range state.
 *
 * `yearStart` / `yearEnd` are local refs initialised to the full extent of the
 * data.  They are kept in sync with the parent's minYear/maxYear props so
 * that programmatic resets (e.g. from the detail panel) are reflected in the UI.
 */
const yearStart = ref(props.minYear);
const yearEnd = ref(props.maxYear);
watch(
  () => [props.minYear, props.maxYear] as const,
  ([mn, mx]) => {
    yearStart.value = mn;
    yearEnd.value = mx;
  }
);
const yearActive = computed(
  () => yearStart.value !== props.minYear || yearEnd.value !== props.maxYear
);

const isolationLabel = computed(() => {
  if (!props.isolationMode) return '';
  const dirText = props.isolationMode.direction === 'ancestry' ? 'Ancestry' : 'Progeny';
  return `${props.isolationMode.label ?? props.isolationMode.nodeId} (${dirText})`;
});

const suggestions = computed<{ label: string; display: string; searchText: string }[]>(() => {
  const q = searchQuery.value.trim().toLowerCase();
  if (!q || q.length < 2) return [];
  return props.paperLabels.filter((p) => p.searchText.includes(q)).slice(0, 8);
});

const maxCitationsLimit = computed(() => Math.max(10, Math.ceil(props.totalNodes / 2)));

const clusters = computed(() => {
  const result: { id: number; color: string; label: string }[] = [];
  for (let i = 0; i < props.clusterCount; i++) {
    result.push({
      id: i,
      color: CITATION_CLUSTER_PALETTE[i % CITATION_CLUSTER_PALETTE.length]!,
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
    searchQuery.value = first.display;
    showSuggestions.value = false;
    emit('locate-paper', first.label);
    // Clear the live-hide filter so the focus dimming from locate-paper
    // takes over on the full graph. Without this, the composite `display`
    // string matches no node attributes and hides every node.
    clearSearchFilter();
  }
}

function selectSuggestion(s: { label: string; display: string; searchText: string }) {
  searchQuery.value = s.display;
  showSuggestions.value = false;
  emit('locate-paper', s.label);
  // Clear the live-hide filter so the focus dimming from locate-paper
  // takes over on the full graph. Without this, the composite `display`
  // string matches no node attributes and hides every node.
  clearSearchFilter();
}

/** Clear the search box and restore all nodes (no live-hide). */
function clearSearch() {
  searchQuery.value = '';
  showSuggestions.value = false;
  clearSearchFilter();
}

/**
 * Emit a filter-change with an empty search string, preserving the other
 * filter values. Used after selecting a paper so the graph returns to full
 * visibility (with focus dimming applied by the parent via locate-paper).
 */
function clearSearchFilter() {
  emit('filter-change', {
    minCitations: minCitations.value,
    showIsolated: showIsolated.value,
    search: '',
    yearRange: yearActive.value ? [yearStart.value, yearEnd.value] : null,
  });
}

function clearSuggestions() {
  showSuggestions.value = false;
}

function toggleIsolated() {
  showIsolated.value = !showIsolated.value;
  emitFilters();
}

function toggleUnmatched() {
  showUnmatched.value = !showUnmatched.value;
  // The parent owns re-fetching the network when this flag changes, because
  // adding/removing unmatched leaves requires a backend round-trip.
  emit('unmatched-change', showUnmatched.value);
}

function toggleMainPath() {
  emit('main-path-change', !props.showMainPath);
}

function emitFilters() {
  emit('filter-change', {
    minCitations: minCitations.value,
    showIsolated: showIsolated.value,
    search: searchQuery.value,
    yearRange: yearActive.value ? [yearStart.value, yearEnd.value] : null,
  });
}

/**
 * Clamp the two handles so they never cross.  Runs on every `input` event.
 */
function onYearInput() {
  if (yearStart.value > yearEnd.value) {
    const tmp = yearStart.value;
    yearStart.value = yearEnd.value;
    yearEnd.value = tmp;
  }
  emit('year-range-input', [yearStart.value, yearEnd.value], {
    minCitations: minCitations.value,
    showIsolated: showIsolated.value,
    search: searchQuery.value,
  });
}

/**
 * Fired on slider release (`change`).  Commits the final range so the parent
 * can trigger a ForceAtlas2 re-layout on the now-filtered subgraph.
 */
function onYearChange() {
  emit('year-range-commit', [yearStart.value, yearEnd.value]);
  emitFilters();
}

function clearYearSlice() {
  yearStart.value = props.minYear;
  yearEnd.value = props.maxYear;
  emit('year-range-input', [yearStart.value, yearEnd.value], {
    minCitations: minCitations.value,
    showIsolated: showIsolated.value,
    search: searchQuery.value,
  });
  emit('year-range-commit', [yearStart.value, yearEnd.value]);
  emitFilters();
}

function onExport(format: 'png' | 'gexf') {
  showExportMenu.value = false;
  emit('export-image', format);
}

function resetAnalysis() {
  minCitations.value = 0;
  showIsolated.value = true;
  searchQuery.value = '';
  showSuggestions.value = false;
  yearStart.value = props.minYear;
  yearEnd.value = props.maxYear;
  emit('year-range-input', [yearStart.value, yearEnd.value], {
    minCitations: minCitations.value,
    showIsolated: showIsolated.value,
    search: searchQuery.value,
  });
  emit('year-range-commit', [yearStart.value, yearEnd.value]);
  emitFilters();
  emit('reset-analysis');
}
</script>

<style scoped>
/* Dual-handle range slider.
 * Two overlapping native <input type="range"> elements share the same track.
 * Pointer-events are disabled on the track but enabled on the thumbs, so each
 * handle is independently draggable without blocking the other.
 */
.dual-range {
  pointer-events: none;
  background: transparent;
  appearance: none;
  -webkit-appearance: none;
}
.dual-range::-webkit-slider-thumb {
  pointer-events: auto;
  appearance: none;
  -webkit-appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  border: 2px solid #6366f1; /* indigo-500 */
  box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  cursor: pointer;
  position: relative;
  z-index: 2;
}
.dual-range::-moz-range-thumb {
  pointer-events: auto;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  border: 2px solid #6366f1;
  box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  cursor: pointer;
  position: relative;
  z-index: 2;
}
.dual-range::-webkit-slider-runnable-track {
  background: transparent;
}
.dual-range::-moz-range-track {
  background: transparent;
}
</style>
