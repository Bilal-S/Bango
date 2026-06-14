<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import Graph from 'graphology';
import { open } from '@tauri-apps/plugin-dialog';
import CitationNetworkGraph from '../components/citation-network-graph.vue';
import type { IsolationDirection } from '../components/citation-network-graph.vue';
import CitationControls from '../components/citation-controls.vue';
import CitationPaperDetailPanel from '../components/citation-paper-detail-panel.vue';
import ArticleDetailPanel from '../components/article-detail-panel.vue';
import { useCitationNetwork } from '../composables/use-citation-network';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { useMainPathWorker } from '../composables/use-main-path-worker';
import { useArticleSearch } from '../composables/use-article-search';
import { useToast } from '../composables/use-toast';
import { exportNetworkPng, exportNetworkGexf } from '../utils/network-export';
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

const { isLayouting, applyLayout } = useNetworkLayout();
const { applyCitationGraphFilters } = useSigmaRenderer();
const toast = useToast();

/**
 * Article detail panel (opened via "open linked record" from the citation
 * paper detail panel).  Re-uses the same ArticleDetailPanel used in the
 * Articles view so the user sees the full record with notes/tags/labels etc.
 *
 * Flow: click open_in_new in citation panel → ArticleDetailPanel opens over
 * the citation panel → on close, the citation panel is shown again.
 */
const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  moveArticle,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();

const showArticleDetail = ref(false);
const isArticleDetailFullScreen = ref(false);

const graphRef = ref<InstanceType<typeof CitationNetworkGraph> | null>(null);

function locateNode(nodeId: string) {
  graphRef.value?.locateNode(nodeId);
}
function resetZoom() {
  graphRef.value?.resetZoom();
}
function refresh() {
  graphRef.value?.refresh();
}

/**
 * Phase 3 — Main Path (SPC): the worker composable manages the Web Worker
 * lifecycle and exposes reactive node/edge ID sets for highlighting.
 */
const {
  mainPathNodes,
  mainPathEdges,
  computing: mainPathComputing,
  compute: computeMainPath,
  clear: clearMainPath,
} = useMainPathWorker(graph);

const selectedPaper = ref<CitationNode | null>(null);
const focusedNodeId = ref<string | null>(null);
const visibleNodeCount = ref(0);
const visibleEdgeCount = ref(0);
const colorMode = ref<'cluster' | 'temporal'>('cluster');
const layoutMode = ref<'fixed' | 'dynamic'>('fixed');
const selectedClusters = ref<number[]>([]);
const sidebarCollapsed = ref(false);

/**
 * Isolation mode: when active, the graph dims all nodes except the selected
 * paper and its ancestry (papers it cites) or progeny (papers citing it).
 * `null` = no isolation (normal view).
 */
const isolationMode = ref<{ nodeId: string; direction: IsolationDirection; label?: string } | null>(
  null
);

/**
 * Whether unmatched reference papers should be requested from the backend and
 * rendered as dashed grey leaf nodes.  Toggling this triggers a network re-fetch.
 */
const showUnmatched = ref(false);

/**
 * Phase 3 — Main Path (SPC): when true, the graph highlights the main path
 * backbone and dims all other nodes/edges.  Toggling triggers the worker.
 */
const showMainPath = ref(false);

/**
 * Diagnostic empty-state: when there are included articles and reference papers
 * but zero edges, the citation graph cannot be drawn because no references have
 * been matched to included articles.  Surface a clear explanation to the user.
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

const recalculateTrigger = ref(0);

/** Derive cluster count from graph node attributes (visible nodes only). */
const clusterCount = computed(() => {
  void recalculateTrigger.value;
  void visibleNodeCount.value;
  if (!graph.value) return 0;
  const clusters = new Set<number>();
  graph.value.forEachNode((node) => {
    const isHidden = graph.value!.getNodeAttribute(node, 'hidden') as boolean | null;
    if (isHidden !== true) {
      const c = graph.value!.getNodeAttribute(node, 'cluster') as number | null;
      if (c !== null && c !== undefined) clusters.add(c);
    }
  });
  return clusters.size;
});

/** Year range from graph for temporal color gradient. */
const yearRange = computed(() => {
  if (!graph.value) return { min: 2000, max: 2024 };
  let min = Infinity;
  let max = -Infinity;
  graph.value.forEachNode((node) => {
    const yr = graph.value!.getNodeAttribute(node, 'year') as number | null;
    if (yr !== null && yr !== undefined) {
      if (yr < min) min = yr;
      if (yr > max) max = yr;
    }
  });
  if (min === Infinity || max === -Infinity) {
    return { min: 2000, max: 2024 };
  }
  if (min === max) {
    return { min: min - 1, max: min + 1 };
  }
  return { min: Math.floor(min), max: Math.ceil(max) };
});

const stats = computed(() => ({
  totalNodes: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleNodes: visibleNodeCount.value || nodeCount.value,
  visibleEdges: visibleEdgeCount.value || edgeCount.value,
  clusterCount: clusterCount.value,
}));

/** Paper labels for autocomplete search. */
const paperLabels = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id: string) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return attrs.label ?? id;
  });
});

/** Paper titles for autocomplete suggestions. */
const paperTitles = computed(() => {
  const map = new Map<string, string>();
  if (!graph.value) return map;
  for (const id of graph.value.nodes()) {
    const attrs = graph.value.getNodeAttributes(id);
    const label = attrs.label ?? id;
    const title = attrs.title ?? '';
    if (title) map.set(label, title);
  }
  return map;
});

/** Citing papers for detail panel (incoming edges = papers that cite this one). */
const citingPapers = computed(() => {
  if (!selectedPaper.value) return [];
  return getCitingPapers(selectedPaper.value.id).map((id) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return { id, label: attrs.label ?? id };
  });
});

/** Cited papers for detail panel (outgoing edges = papers this one cites). */
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
  focusedNodeId.value = nodeId;
  if (!nodeId) {
    // Closing the panel (via close button or graph background click) also
    // resets isolation to "Show All" so the graph returns to full brightness.
    selectedPaper.value = null;
    isolationMode.value = null;
    return;
  }
  selectedPaper.value = getNode(nodeId);
}

function onNavigateToPaper(nodeId: string) {
  onNodeClick(nodeId);
  locateNode(nodeId);
}

/**
 * Enter isolation mode for the currently-selected paper.
 * Called from the detail panel's "Isolate Ancestry" / "Isolate Progeny" buttons.
 */
function onIsolate(direction: IsolationDirection) {
  if (!selectedPaper.value) return;
  isolationMode.value = {
    nodeId: selectedPaper.value.id,
    direction,
    label: selectedPaper.value.label,
  };
}

/** Exit isolation mode and return to the normal full-brightness view. */
function onClearIsolation() {
  isolationMode.value = null;
}

/**
 * Open the full article detail panel from the citation paper detail panel.
 * The linked record shares the same ID as the citation node (for matched/
 * included papers), so we can pass `paper.id` directly to `selectArticle`.
 *
 * On close, we keep `selectedPaper` intact so the citation panel re-appears.
 */
async function onOpenLinkedRecord(articleId: string) {
  try {
    await selectArticle(articleId);
    showArticleDetail.value = true;
  } catch {
    toast.show('Failed to load article details', 'error');
  }
}

/** Close the article detail panel and return to the citation paper detail. */
function onCloseArticleDetail() {
  showArticleDetail.value = false;
  isArticleDetailFullScreen.value = false;
  detailArticle.value = null;
  detailAuditTrail.value = [];
}

/** Attach a full-text PDF/text file to the article shown in the detail panel. */
async function handleAttachFullText(articleId: string): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'Documents', extensions: ['pdf', 'txt'] }],
    });
    if (!selected) return;
    toast.show('Importing full text…', 'info');
    await attachFullText(articleId, selected);
    toast.show('Full text attached successfully.', 'success');
  } catch {
    toast.show('Failed to attach full text', 'error');
  }
}

// Layout mode change triggers a full recalculate under the new layout strategy
async function onLayoutModeChange(mode: 'fixed' | 'dynamic') {
  layoutMode.value = mode;
  if (graph.value) {
    await onRecalculate();
  }
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

/**
 * Phase 2 — Time-Slice: live drag handler.  Applies the year-range filter
 * immediately (hide/show nodes) but does NOT trigger a ForceAtlas2 re-layout.
 * The expensive layout is deferred until the slider is released (commit).
 */
function onYearRangeInput(
  range: [number, number],
  otherFilters?: { minCitations: number; showIsolated: boolean; search: string }
) {
  debouncedApplyFilters(range, otherFilters ?? { minCitations: 0, showIsolated: true, search: '' });
}

/**
 * Phase 2 — Time-Slice: commit handler (slider release).  Triggers the full
 * ForceAtlas2 re-layout on the now-filtered subgraph.
 */
async function onYearRangeCommit(_range: [number, number]) {
  await onRecalculate();
}

/**
 * Handle the "Show Unmatched References" toggle.  Toggling requires a backend
 * round-trip (unmatched leaves are added/removed server-side), so we re-fetch
 * the network and re-apply the layout.
 */
async function onUnmatchedChange(newShowUnmatched: boolean) {
  showUnmatched.value = newShowUnmatched;
  selectedPaper.value = null;
  focusedNodeId.value = null;
  selectedClusters.value = [];
  await fetchNetwork(showUnmatched.value);
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
    // Re-run main path on the new graph if the highlight is currently active.
    if (showMainPath.value) computeMainPath();
  }
}

/**
 * Phase 3 — Main Path (SPC): toggle handler.  When turning on, trigger the
 * worker computation.  When turning off, clear the highlight state.
 */
function onMainPathChange(newShowMainPath: boolean) {
  showMainPath.value = newShowMainPath;
  if (newShowMainPath) {
    computeMainPath();
  } else {
    clearMainPath();
  }
}

function onLocatePaper(label: string) {
  if (!graph.value) return;
  const nodeId = graph.value.findNode(
    (node) => (graph.value!.getNodeAttribute(node, 'label') as string) === label
  );
  if (nodeId) {
    onNodeClick(nodeId);
    locateNode(nodeId);
  }
}

/** Export network in chosen format (PNG or GEXF) via Tauri save dialog. */
async function onExportImage(format: NetworkExportFormat) {
  try {
    if (format === 'png') {
      if (!graphRef.value?.renderer) return;
      await exportNetworkPng(graphRef.value.renderer, 'citation-network.png');
    } else if (format === 'gexf') {
      if (!graph.value) return;
      await exportNetworkGexf(graph.value, 'citation-network.gexf');
    }
  } catch (err) {
    console.error('[export] Citation network export failed:', err);
  }
}

function onSelectCluster(clusterId: number) {
  const idx = selectedClusters.value.indexOf(clusterId);
  if (idx >= 0) {
    selectedClusters.value.splice(idx, 1);
  } else {
    selectedClusters.value.push(clusterId);
  }
}

function onClearClusters() {
  selectedClusters.value = [];
}

async function onResetAnalysis() {
  colorMode.value = 'cluster';
  layoutMode.value = 'fixed';
  selectedClusters.value = [];
  selectedPaper.value = null;
  focusedNodeId.value = null;
  await onRecalculate();
}

async function onRecalculate() {
  if (!graph.value) return;

  isLayouting.value = true;
  try {
    // 1. Create a temporary directed subgraph of visible nodes and edges
    const sub = new Graph({ type: 'directed', multi: false });

    graph.value.forEachNode((node, attrs) => {
      if (attrs.hidden !== true) {
        sub.addNode(node, { ...attrs });
      }
    });

    graph.value.forEachEdge((edge, attrs, source, target) => {
      if (attrs.hidden !== true && sub.hasNode(source) && sub.hasNode(target)) {
        sub.addDirectedEdgeWithKey(edge, source, target, { ...attrs });
      }
    });

    if (sub.order === 0) return;

    // 2. Run Louvain and ForceAtlas2 layout on the subgraph
    await applyLayout(sub, 100, layoutMode.value);

    // 3. Write back coordinates and cluster to the parent graph
    sub.forEachNode((node) => {
      const newAttrs = sub.getNodeAttributes(node);
      graph.value!.setNodeAttribute(node, 'x', newAttrs.x);
      graph.value!.setNodeAttribute(node, 'y', newAttrs.y);
      graph.value!.setNodeAttribute(node, 'cluster', newAttrs.cluster);
    });

    // 4. Force Sigma renderer to update and zoom to fit
    resetZoom();
    refresh();

    // 5. Trigger computed statistics updates
    recalculateTrigger.value++;
  } finally {
    isLayouting.value = false;
  }
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
          :paper-titles="paperTitles"
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

    <!-- Full article detail panel (opened from the citation paper detail panel).
         On close, returns to the citation sidebar.  Supports full-screen
         expansion which hides the graph canvas. -->
    <Transition name="detail-slide">
      <ArticleDetailPanel
        v-if="showArticleDetail && detailArticle"
        :article="detailArticle"
        :audit-trail="detailAuditTrail"
        :has-previous="false"
        :has-next="false"
        :has-return-target="false"
        :full-screen="isArticleDetailFullScreen"
        :article-position="1"
        :article-total="1"
        @close="onCloseArticleDetail"
        @toggle-full-screen="isArticleDetailFullScreen = !isArticleDetailFullScreen"
        @update-notes="updateNotes"
        @update-tags="updateTags"
        @update-labels="updateLabels"
        @update-criteria="updateCriteria"
        @move-article="moveArticle"
        @attach-full-text="handleAttachFullText"
        @delete-full-text="deleteFullTextAttachment"
        @refresh-article="selectArticle"
      />
    </Transition>
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
  background: #f8fafc;
  border: 1px solid #e2e8f0;
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
  background: #eef2ff;
  border-color: #a5b4fc;
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
  background: #6366f1;
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
