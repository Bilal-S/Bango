<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import CitationNetworkGraph from '../components/citation-network-graph.vue';
import type { IsolationDirection } from '../components/citation-network-graph.vue';
import CitationControls from '../components/citation-controls.vue';
import CitationPaperDetailPanel from '../components/citation-paper-detail-panel.vue';
import ArticleDetailSlideOver from '../components/article-detail-slide-over.vue';
import { useCitationNetwork } from '../composables/use-citation-network';
import { useNetworkView } from '../composables/use-network-view';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { useMainPathWorker } from '../composables/use-main-path-worker';
import { debounce } from '../utils/debounce';
import type { NetworkExportFormat } from '../utils/network-export';
import type { CitationNode } from '../types/biblio-citation';

const {
  graph,
  loading,
  error,
  meta,
  nodeCount,
  edgeCount,
  fetchNetwork,
  getNode,
  getCitingPapers,
  getCitedPapers,
} = useCitationNetwork();

const {
  graphRef,
  focusedNodeId,
  visibleNodeCount,
  visibleEdgeCount,
  colorMode,
  layoutMode,
  selectedClusters,
  sidebarCollapsed,
  recalculateTrigger,
  clusterCount,
  yearRange,
  isLayouting,
  applyLayout,
  focusNode,
  locateByLabel,
  onSelectCluster,
  onClearClusters,
  onLayoutModeChange,
  onExportImage: exportImage,
  onRecalculate,
  resetViewState,
} = useNetworkView({
  graph,
  exportPrefix: 'citation-network',
  graphType: 'directed',
});

const { applyCitationGraphFilters } = useSigmaRenderer();

/**
 * Article detail panel (opened via "open linked record" from citation paper
 * detail). Shared `ArticleDetailSlideOver` owns the useArticleSearch wiring +
 * panel lifecycle; this view keeps only the overlay guards.
 */
const articleDetailRef = ref<InstanceType<typeof ArticleDetailSlideOver> | null>(null);
const showArticleDetail = ref(false);
const isArticleDetailFullScreen = ref(false);

function onArticleDetailOpened(): void {
  showArticleDetail.value = true;
}

function onArticleDetailClosed(): void {
  showArticleDetail.value = false;
  isArticleDetailFullScreen.value = false;
}

function onArticleDetailToggleFullScreen(): void {
  isArticleDetailFullScreen.value = !isArticleDetailFullScreen.value;
}

const selectedPaper = ref<CitationNode | null>(null);

// Isolation mode: dims all nodes except selected paper + ancestry/progeny.
const isolationMode = ref<{ nodeId: string; direction: IsolationDirection; label?: string } | null>(
  null
);

/** Whether unmatched reference papers should be requested and rendered. */
const showUnmatched = ref(false);

/** Phase 3 - Main Path (SPC) highlight toggle. */
const showMainPath = ref(false);

const {
  mainPathNodes,
  mainPathEdges,
  computing: mainPathComputing,
  compute: computeMainPath,
  clear: clearMainPath,
} = useMainPathWorker(graph);

/**
 * Diagnostic empty-state: included articles + reference papers but zero edges
 * means no references have been matched to included articles.
 */
const isEmptyDueToNoMatches = computed(() => {
  const m = meta.value;
  if (!m) return false;
  return (
    m.includedArticleCount > 0 &&
    m.referencePaperCount > 0 &&
    m.matchedPaperCount === 0 &&
    m.edgeCount === 0
  );
});

const hasReferencePapers = computed(() => (meta.value?.referencePaperCount ?? 0) > 0);

const stats = computed(() => ({
  totalNodes: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleNodes: visibleNodeCount.value || nodeCount.value,
  visibleEdges: visibleEdgeCount.value || edgeCount.value,
  clusterCount: clusterCount.value,
}));

/** Paper search entries for autocomplete search. */
const paperLabels = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id: string) => {
    const attrs = graph.value!.getNodeAttributes(id);
    const label = (attrs.label as string) ?? id;
    const title = (attrs.title as string) ?? '';
    const authors = (attrs.authors as string) ?? '';
    const doi = (attrs.doi as string) ?? '';
    const shortTitle = title.length > 15 ? title.slice(0, 12) + '…' : title;
    return {
      label,
      display: shortTitle ? `${shortTitle}:${label}` : label,
      searchText: [label, title, authors, doi].join(' ').toLowerCase(),
    };
  });
});

/** Citing papers for detail panel (incoming edges). */
const citingPapers = computed(() => {
  if (!selectedPaper.value) return [];
  return getCitingPapers(selectedPaper.value.id).map((id) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return { id, label: attrs.label ?? id };
  });
});

/** Cited papers for detail panel (outgoing edges). */
const citedPapers = computed(() => {
  if (!selectedPaper.value) return [];
  return getCitedPapers(selectedPaper.value.id).map((id) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return { id, label: attrs.label ?? id };
  });
});

onMounted(async () => {
  await fetchNetwork();
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
});

function onNodeClick(nodeId: string | null) {
  focusNode(nodeId);
  if (!nodeId) {
    selectedPaper.value = null;
    isolationMode.value = null;
    return;
  }
  selectedPaper.value = getNode(nodeId);
}

function onNavigateToPaper(nodeId: string) {
  onNodeClick(nodeId);
  graphRef.value?.locateNode(nodeId);
}

/**
 * Handle the sidebar autocomplete "locate paper" event: focus + pan/zoom to
 * the node AND open the detail panel. `locateByLabel` returns the node id when
 * a match is found; we feed it into the same node-selection path as a direct
 * click so the panel + isolation context is consistent.
 */
function onLocatePaper(label: string) {
  const nodeId = locateByLabel(label);
  if (nodeId) {
    selectedPaper.value = getNode(nodeId);
  }
}

/** Enter isolation mode for the currently-selected paper. */
function onIsolate(direction: IsolationDirection) {
  if (!selectedPaper.value) return;
  isolationMode.value = {
    nodeId: selectedPaper.value.id,
    direction,
    label: selectedPaper.value.label,
  };
}

/** Exit isolation mode. */
function onClearIsolation() {
  isolationMode.value = null;
}

/** Open the full article detail panel from the citation paper detail panel. */
function onOpenLinkedRecord(articleId: string) {
  void articleDetailRef.value?.open(articleId);
}

function onFilterChange(filters: {
  minCitations: number;
  showIsolated: boolean;
  search: string;
  yearRange?: [number, number] | null;
}) {
  if (!graph.value) return;
  const result = applyCitationGraphFilters(graph.value, filters);
  visibleNodeCount.value = result.visibleNodes;
  visibleEdgeCount.value = result.visibleEdges;
  if (showMainPath.value) {
    computeMainPath();
  }
}

const debouncedApplyFilters = debounce(
  (
    range: [number, number],
    otherFilters: { minCitations: number; showIsolated: boolean; search: string }
  ) => {
    if (!graph.value) return;
    const result = applyCitationGraphFilters(graph.value, {
      ...otherFilters,
      yearRange: range,
    });
    visibleNodeCount.value = result.visibleNodes;
    visibleEdgeCount.value = result.visibleEdges;
    if (showMainPath.value) {
      computeMainPath();
    }
  },
  50
);

/** Time-Slice live drag handler (hide/show nodes, no re-layout). */
function onYearRangeInput(
  range: [number, number],
  otherFilters?: { minCitations: number; showIsolated: boolean; search: string }
) {
  debouncedApplyFilters(range, otherFilters ?? { minCitations: 0, showIsolated: true, search: '' });
}

/** Time-Slice commit handler (slider release -> full re-layout). */
async function onYearRangeCommit(_range: [number, number]) {
  await onRecalculate();
}

/** Handle the "Show Unmatched References" toggle (backend round-trip). */
async function onUnmatchedChange(newShowUnmatched: boolean) {
  showUnmatched.value = newShowUnmatched;
  selectedPaper.value = null;
  focusNode(null);
  selectedClusters.value = [];
  await fetchNetwork(showUnmatched.value);
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
    if (showMainPath.value) computeMainPath();
  }
}

/** Main Path (SPC) toggle handler. */
function onMainPathChange(newShowMainPath: boolean) {
  showMainPath.value = newShowMainPath;
  if (newShowMainPath) {
    computeMainPath();
  } else {
    clearMainPath();
  }
}

async function onExportImage(format: NetworkExportFormat) {
  const renderer = (graphRef.value as { renderer?: unknown } | null)?.renderer;
  await exportImage(format, (renderer as Parameters<typeof exportImage>[1]) ?? null);
}

async function onResetAnalysis() {
  resetViewState();
  selectedPaper.value = null;
  await onRecalculate();
}
</script>

<template>
  <div class="citation-layout">
    <!-- Sidebar Wrapper -->
    <div
      class="sidebar-wrapper relative transition-all duration-300 shrink-0"
      :class="sidebarCollapsed ? 'w-0' : 'w-64'"
    >
      <!-- Controls sidebar -->
      <aside
        class="sidebar-panel h-full p-4 overflow-y-auto border-r border-slate-100 bg-slate-50/30 transition-all duration-300 flex flex-col"
        :class="sidebarCollapsed ? 'w-0 p-0 overflow-hidden opacity-0' : 'w-full opacity-100'"
      >
        <CitationControls
          class="my-auto"
          :total-nodes="stats.totalNodes"
          :total-edges="stats.totalEdges"
          :visible-nodes="stats.visibleNodes"
          :visible-edges="stats.visibleEdges"
          :cluster-count="stats.clusterCount"
          :paper-labels="paperLabels"
          :color-mode="colorMode"
          :layout-mode="layoutMode"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :selected-clusters="selectedClusters"
          :show-unmatched="showUnmatched"
          :show-main-path="showMainPath"
          :isolation-mode="isolationMode"
          @main-path-change="onMainPathChange"
          @filter-change="onFilterChange"
          @locate-paper="onLocatePaper"
          @export-image="onExportImage"
          @color-mode-change="colorMode = $event"
          @layout-mode-change="onLayoutModeChange"
          @select-cluster="onSelectCluster($event)"
          @clear-clusters="onClearClusters"
          @recalculate="onRecalculate"
          @reset-analysis="onResetAnalysis"
          @unmatched-change="onUnmatchedChange"
          @year-range-input="onYearRangeInput"
          @year-range-commit="onYearRangeCommit"
          @clear-isolation="onClearIsolation"
        />
      </aside>

      <!-- Drawer handle -->
      <button
        class="drawer-handle"
        :title="sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'"
        :style="{ left: sidebarCollapsed ? '0px' : 'calc(100% - 16px)' }"
        @click="sidebarCollapsed = !sidebarCollapsed"
      >
        <span class="drawer-handle-grip"></span>
      </button>
    </div>

    <!-- Graph canvas (hidden when article detail is fullscreen) -->
    <main v-show="!(showArticleDetail && isArticleDetailFullScreen)" class="flex-1 relative">
      <CitationNetworkGraph
        ref="graphRef"
        :graph="graph"
        :loading="loading"
        :is-layouting="isLayouting || mainPathComputing"
        :error="error"
        :focused-node-id="focusedNodeId"
        :selected-clusters="selectedClusters"
        :color-mode="colorMode"
        :min-year="yearRange.min"
        :max-year="yearRange.max"
        :recalculate-trigger="recalculateTrigger"
        :isolation-mode="isolationMode"
        :main-path-nodes="mainPathNodes"
        :main-path-edges="mainPathEdges"
        :show-main-path="showMainPath"
        @node-click="onNodeClick"
        @retry="fetchNetwork"
      />

      <!-- Diagnostic empty-state: references exist but none matched to articles -->
      <div
        v-if="!loading && !error && isEmptyDueToNoMatches"
        class="absolute bottom-4 left-1/2 -translate-x-1/2 z-30 max-w-md bg-amber-50 border border-amber-200 rounded-xl shadow-sm p-4 flex gap-3"
      >
        <span class="material-symbols-outlined text-amber-500 text-xl shrink-0">info</span>
        <div class="text-xs text-amber-800">
          <p class="font-semibold mb-0.5">No citation edges found</p>
          <p class="leading-relaxed">
            {{ meta?.referencePaperCount }} reference papers were imported, but none have been
            matched to your {{ meta?.includedArticleCount }} included articles. Citation edges are
            only drawn between included articles.
          </p>
          <p class="mt-1.5 leading-relaxed text-amber-700">
            Try enabling
            <span class="font-medium">"Show Unmatched References"</span> in the sidebar to see all
            reference papers as disconnected leaves.
          </p>
        </div>
      </div>

      <!-- Diagnostic empty-state: no reference papers imported at all -->
      <div
        v-if="!loading && !error && !hasReferencePapers && stats.totalNodes > 0"
        class="absolute bottom-4 left-1/2 -translate-x-1/2 z-30 max-w-md bg-blue-50 border border-blue-200 rounded-xl shadow-sm p-4 flex gap-3"
      >
        <span class="material-symbols-outlined text-blue-500 text-xl shrink-0">info</span>
        <div class="text-xs text-blue-800">
          <p class="font-semibold mb-0.5">No references imported</p>
          <p class="leading-relaxed">
            No reference papers have been extracted from your articles yet. Import articles with
            embedded references (e.g. RIS files with N1/N2/abstract fields) to build a citation
            network.
          </p>
        </div>
      </div>
    </main>

    <!-- Citation detail panel (hidden while the article detail is open) -->
    <Transition name="detail-slide">
      <CitationPaperDetailPanel
        v-if="selectedPaper && !showArticleDetail"
        :paper="selectedPaper"
        :citing-papers="citingPapers"
        :cited-papers="citedPapers"
        :isolation-mode="isolationMode"
        :main-path-nodes="mainPathNodes"
        class="w-72 shrink-0"
        @close="onNodeClick(null)"
        @navigate-paper="onNavigateToPaper"
        @isolate="onIsolate"
        @clear-isolation="onClearIsolation"
        @open-linked-record="onOpenLinkedRecord"
      />
    </Transition>

    <!-- Full article detail panel (opened from the citation paper detail panel). -->
    <ArticleDetailSlideOver
      ref="articleDetailRef"
      :full-screen="isArticleDetailFullScreen"
      @opened="onArticleDetailOpened"
      @closed="onArticleDetailClosed"
      @toggle-full-screen="onArticleDetailToggleFullScreen"
    />
  </div>
</template>

<style scoped>
.citation-layout {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}

.sidebar-panel {
  z-index: 20;
}

/* Drawer handle - small pill tab positioned at sidebar edge */
.drawer-handle {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 30;
  width: 14px;
  height: 72px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-surface-container-low);
  border: 1px solid var(--color-outline-variant);
  border-left: none;
  border-radius: 0 8px 8px 0;
  box-shadow: 2px 0 4px rgba(0, 0, 0, 0.06);
  cursor: pointer;
  transition:
    left 0.3s,
    background-color 0.15s,
    border-color 0.15s,
    width 0.15s;
}

.drawer-handle:hover {
  background: var(--color-surface-container);
  border-color: var(--color-primary);
  width: 16px;
}

/* Grip dots inside the handle */
.drawer-handle-grip {
  display: flex;
  flex-direction: column;
  gap: 3px;
  align-items: center;
}

.drawer-handle-grip::before,
.drawer-handle-grip::after,
.drawer-handle-grip {
  content: '';
  display: block;
  width: 4px;
  height: 2px;
  border-radius: 1px;
  background: #94a3b8;
  transition: background-color 0.15s;
}

.drawer-handle:hover .drawer-handle-grip::before,
.drawer-handle:hover .drawer-handle-grip::after,
.drawer-handle:hover .drawer-handle-grip {
  background: var(--color-primary);
}

/* Detail panel slide transition */
.detail-slide-enter-active,
.detail-slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}

.detail-slide-enter-from,
.detail-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
