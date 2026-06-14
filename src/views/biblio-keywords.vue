<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import Graph from 'graphology';
import KeywordNetworkGraph from '../components/keyword-network-graph.vue';
import KeywordControls from '../components/keyword-controls.vue';
import KeywordDetailPanel from '../components/keyword-detail-panel.vue';
import { useKeywordNetwork } from '../composables/use-keyword-network';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { exportNetworkPng, exportNetworkGexf } from '../utils/network-export';
import type { NetworkExportFormat } from '../utils/network-export';
import type { KeywordNode } from '../types/biblio-keyword';

const {
  graph,
  loading,
  error,
  nodeCount,
  edgeCount,
  clusterCount,
  isLayouting,
  sources,
  minOccurrences,
  minCooccurrence,
  fetchNetwork,
} = useKeywordNetwork();

const { applyLayout } = useNetworkLayout();
const { applyKeywordGraphFilters } = useSigmaRenderer();

const graphRef = ref<InstanceType<typeof KeywordNetworkGraph> | null>(null);

function locateNode(nodeId: string) {
  graphRef.value?.locateNode(nodeId);
}
function resetZoom() {
  graphRef.value?.resetZoom();
}
function refresh() {
  graphRef.value?.refresh();
}

const selectedKeyword = ref<KeywordNode | null>(null);
const focusedNodeId = ref<string | null>(null);
const visibleNodeCount = ref(0);
const visibleEdgeCount = ref(0);
const colorMode = ref<'cluster' | 'temporal'>('cluster');
const layoutMode = ref<'fixed' | 'dynamic'>('fixed');
const selectedClusters = ref<number[]>([]);
const sidebarCollapsed = ref(false);
const recalculateTrigger = ref(0);

/** Year range from graph nodes' avgYear for temporal color gradient */
const yearRange = computed(() => {
  if (!graph.value) return { min: 2000, max: 2026 };
  let min = Infinity;
  let max = -Infinity;
  graph.value.forEachNode((node) => {
    const yr = graph.value!.getNodeAttribute(node, 'avgYear') as number | null;
    if (yr !== null && yr !== undefined) {
      if (yr < min) min = yr;
      if (yr > max) max = yr;
    }
  });
  if (min === Infinity || max === -Infinity) {
    return { min: 2000, max: 2026 };
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

/** Keyword labels for autocomplete search */
const keywordLabels = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return attrs.label ?? id;
  });
});

onMounted(async () => {
  await fetchNetwork(layoutMode.value);
  if (graph.value) {
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
});

function onNodeClick(nodeId: string | null) {
  focusedNodeId.value = nodeId;
  if (!nodeId) {
    selectedKeyword.value = null;
    return;
  }
  if (graph.value && graph.value.hasNode(nodeId)) {
    const attrs = graph.value.getNodeAttributes(nodeId);
    selectedKeyword.value = {
      id: nodeId,
      label: attrs.label ?? nodeId,
      weight: attrs.weight ?? 0,
      source: attrs.source ?? '',
      avgYear: attrs.avgYear ?? null,
      rawTerms: attrs.rawTerms ?? [],
      cluster: attrs.cluster ?? null,
      yearCounts: attrs.yearCounts ?? [],
    };
  }
}

function onNavigateToKeyword(nodeId: string) {
  onNodeClick(nodeId);
  locateNode(nodeId);
}

async function onParamsChange(params: {
  sources: string[];
  minOccurrences: number;
  minCooccurrence: number;
}) {
  sources.value = params.sources;
  minOccurrences.value = params.minOccurrences;
  minCooccurrence.value = params.minCooccurrence;

  selectedKeyword.value = null;
  focusedNodeId.value = null;
  selectedClusters.value = [];

  await fetchNetwork(layoutMode.value);
  if (graph.value) {
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
}

async function onLayoutModeChange(mode: 'fixed' | 'dynamic') {
  layoutMode.value = mode;
  if (graph.value) {
    await onRecalculate();
  }
}

function onFilterChange(filters: {
  minOccurrences: number;
  minCooccurrence: number;
  search: string;
}) {
  if (!graph.value) return;
  const result = applyKeywordGraphFilters(graph.value, filters);
  visibleNodeCount.value = result.visibleNodes;
  visibleEdgeCount.value = result.visibleEdges;
}

/** Export network in chosen format (PNG or GEXF) via Tauri save dialog */
async function onExportImage(format: NetworkExportFormat) {
  try {
    if (format === 'png') {
      if (!graphRef.value?.renderer) return;
      await exportNetworkPng(graphRef.value.renderer);
    } else if (format === 'gexf') {
      if (!graph.value) return;
      await exportNetworkGexf(graph.value);
    }
  } catch (err) {
    console.error('[export] Keyword network export failed:', err);
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
  selectedKeyword.value = null;
  focusedNodeId.value = null;

  // Set defaults and re-fetch
  sources.value = ['metadata', 'ai_extracted', 'tags', 'labels', 'user_added'];
  minOccurrences.value = 2;
  minCooccurrence.value = 2;

  await fetchNetwork(layoutMode.value);
  if (graph.value) {
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
}

async function onRecalculate() {
  if (!graph.value) return;

  isLayouting.value = true;
  try {
    // 1. Create a temporary undirected subgraph of visible nodes and edges
    const sub = new Graph({ type: 'undirected', multi: false });

    graph.value.forEachNode((node, attrs) => {
      if (attrs.hidden !== true) {
        sub.addNode(node, { ...attrs });
      }
    });

    graph.value.forEachEdge((edge, attrs, source, target) => {
      if (attrs.hidden !== true && sub.hasNode(source) && sub.hasNode(target)) {
        sub.addUndirectedEdgeWithKey(edge, source, target, { ...attrs });
      }
    });

    if (sub.order === 0) return;

    // 2. Run Louvain and ForceAtlas2 layout on the subgraph
    await applyLayout(sub, 150, layoutMode.value);

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

function onLocateKeyword(label: string) {
  if (!graph.value) return;
  const nodeId = graph.value.findNode(
    (node) => (graph.value!.getNodeAttribute(node, 'label') as string) === label
  );
  if (nodeId) {
    onNodeClick(nodeId);
    locateNode(nodeId);
  }
}
</script>

<template>
  <div class="keyword-layout">
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
        <KeywordControls
          class="my-auto"
          :total-nodes="stats.totalNodes"
          :total-edges="stats.totalEdges"
          :visible-nodes="stats.visibleNodes"
          :visible-edges="stats.visibleEdges"
          :cluster-count="stats.clusterCount"
          :keyword-labels="keywordLabels"
          :color-mode="colorMode"
          :layout-mode="layoutMode"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :selected-clusters="selectedClusters"
          :sources="sources"
          :min-occurrences="minOccurrences"
          :min-cooccurrence="minCooccurrence"
          @filter-change="onFilterChange"
          @params-change="onParamsChange"
          @layout-mode-change="onLayoutModeChange"
          @locate-keyword="onLocateKeyword"
          @export-image="onExportImage"
          @color-mode-change="colorMode = $event"
          @select-cluster="onSelectCluster($event)"
          @clear-clusters="onClearClusters"
          @fit-screen="resetZoom"
          @recalculate="onRecalculate"
          @reset-analysis="onResetAnalysis"
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

    <!-- Graph canvas -->
    <main class="flex-1 relative">
      <KeywordNetworkGraph
        ref="graphRef"
        :graph="graph"
        :loading="loading"
        :is-layouting="isLayouting"
        :error="error"
        :focused-node-id="focusedNodeId"
        :selected-clusters="selectedClusters"
        :color-mode="colorMode"
        :min-year="yearRange.min"
        :max-year="yearRange.max"
        :recalculate-trigger="recalculateTrigger"
        @node-click="onNodeClick"
        @retry="fetchNetwork"
      />
    </main>

    <!-- Keyword detail panel -->
    <Transition name="detail-slide">
      <KeywordDetailPanel
        v-if="selectedKeyword"
        :keyword="selectedKeyword"
        :graph="graph"
        class="w-72 shrink-0"
        @close="onNodeClick(null)"
        @navigate="onNavigateToKeyword"
      />
    </Transition>
  </div>
</template>

<style scoped>
.keyword-layout {
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
