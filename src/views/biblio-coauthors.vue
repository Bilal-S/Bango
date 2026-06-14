<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import Graph from 'graphology';
import NetworkGraph from '../components/network-graph.vue';
import NetworkControls from '../components/network-controls.vue';
import AuthorDetailPanel from '../components/author-detail-panel.vue';
import { useCoAuthorNetwork } from '../composables/use-coauthor-network';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { exportNetworkPng, exportNetworkGexf } from '../utils/network-export';
import type { NetworkExportFormat } from '../utils/network-export';
import type { CoAuthorNode } from '../types/biblio-network';

const { graph, loading, error, nodeCount, edgeCount, countingMode, fetchNetwork, setCountingMode } =
  useCoAuthorNetwork();

const { isLayouting, applyLayout, runForceAtlas2Async } = useNetworkLayout();
const { applyGraphFilters } = useSigmaRenderer();
const graphRef = ref<InstanceType<typeof NetworkGraph> | null>(null);

function locateNode(nodeId: string) {
  graphRef.value?.locateNode(nodeId);
}
function resetZoom() {
  graphRef.value?.resetZoom();
}
function refresh() {
  graphRef.value?.refresh();
}

const selectedAuthor = ref<CoAuthorNode | null>(null);
const focusedNodeId = ref<string | null>(null);
const visibleNodeCount = ref(0);
const visibleEdgeCount = ref(0);
const colorMode = ref<'cluster' | 'temporal'>('cluster');
const layoutMode = ref<'fixed' | 'dynamic'>('fixed');
const selectedClusters = ref<number[]>([]);
const sidebarCollapsed = ref(false);

const recalculateTrigger = ref(0);

/** Derive cluster count from graph node attributes */
const clusterCount = computed(() => {
  // Access visibleNodeCount and recalculateTrigger to register reactive dependencies
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

const yearRange = computed(() => {
  if (!graph.value) return { min: 2000, max: 2024 };
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
    return { min: 2000, max: 2024 };
  }
  if (min === max) {
    return { min: min - 1, max: min + 1 };
  }
  return { min: Math.floor(min), max: Math.ceil(max) };
});

const stats = computed(() => ({
  totalAuthors: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleAuthors: visibleNodeCount.value || nodeCount.value,
  visibleEdges: visibleEdgeCount.value || edgeCount.value,
  clusterCount: clusterCount.value,
}));

const authorNames = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id: string) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return attrs.label ?? id;
  });
});

const authorWeights = computed(() => {
  const map = new Map<string, number>();
  if (!graph.value) return map;
  for (const id of graph.value.nodes()) {
    const attrs = graph.value.getNodeAttributes(id);
    map.set(attrs.label ?? id, attrs.weight ?? 0);
  }
  return map;
});

onMounted(async () => {
  await fetchNetwork();
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    // Initialize visible counts
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
});

function onNodeClick(nodeId: string | null) {
  focusedNodeId.value = nodeId;
  if (!nodeId || !graph.value) {
    selectedAuthor.value = null;
    return;
  }
  const attrs = graph.value.getNodeAttributes(nodeId);
  selectedAuthor.value = {
    id: nodeId,
    label: attrs.label ?? nodeId,
    weight: attrs.weight ?? 0,
    totalCitations: attrs.totalCitations ?? 0,
    avgYear: attrs.avgYear ?? null,
    estimatedHIndex: attrs.estimatedHIndex ?? null,
    cluster: attrs.cluster ?? null,
  };
}

function onNavigateToAuthor(nodeId: string) {
  onNodeClick(nodeId);
  locateNode(nodeId);
}

// Layout mode change triggers a full recalculate under the new layout strategy
async function onLayoutModeChange(mode: 'fixed' | 'dynamic') {
  layoutMode.value = mode;
  if (graph.value) {
    await onRecalculate();
  }
}

function onFilterChange(filters: {
  minPapers: number;
  minLinkStrength: number;
  maxAuthors: number;
  search: string;
}) {
  if (!graph.value) return;
  const result = applyGraphFilters(graph.value, filters);
  visibleNodeCount.value = result.visibleNodes;
  visibleEdgeCount.value = result.visibleEdges;
}

function onLocateAuthor(name: string) {
  if (!graph.value) return;
  const nodeId = graph.value.findNode(
    (node) => (graph.value!.getNodeAttribute(node, 'label') as string) === name
  );
  if (nodeId) {
    onNodeClick(nodeId);
    locateNode(nodeId);
  }
}

// Export network in chosen format (PNG or GEXF) via Tauri save dialog
async function onExportImage(format: NetworkExportFormat) {
  try {
    if (format === 'png') {
      if (!graphRef.value?.renderer) return;
      await exportNetworkPng(graphRef.value.renderer, 'coauthor-network.png');
    } else if (format === 'gexf') {
      if (!graph.value) return;
      await exportNetworkGexf(graph.value, 'coauthor-network.gexf');
    }
  } catch (err) {
    console.error('[export] Network export failed:', err);
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

async function onCountingModeChange(mode: 'full' | 'fractional') {
  const changed = setCountingMode(mode);
  if (changed && graph.value) {
    // Quick layout polish after weight change
    await runForceAtlas2Async(graph.value, 30, layoutMode.value);
  }
}

async function onResetAnalysis() {
  // Reset all analysis state to defaults
  colorMode.value = 'cluster';
  layoutMode.value = 'fixed';
  selectedClusters.value = [];
  selectedAuthor.value = null;
  focusedNodeId.value = null;

  // Reset counting mode if changed
  const changed = setCountingMode('full');
  if (changed && graph.value) {
    await runForceAtlas2Async(graph.value, 30, layoutMode.value);
  }

  // Re-apply filters (already reset by controls) and recalculate layout
  await onRecalculate();
}

async function onRecalculate() {
  if (!graph.value) return;

  isLayouting.value = true;
  try {
    // 1. Create a temporary subgraph of visible nodes and edges
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
  <div class="coauthor-layout">
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
        <NetworkControls
          class="my-auto"
          :total-nodes="stats.totalAuthors"
          :total-edges="stats.totalEdges"
          :visible-nodes="stats.visibleAuthors"
          :visible-edges="stats.visibleEdges"
          :cluster-count="stats.clusterCount"
          :author-names="authorNames"
          :author-weights="authorWeights"
          :counting-mode="countingMode"
          :color-mode="colorMode"
          :layout-mode="layoutMode"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :selected-clusters="selectedClusters"
          @filter-change="onFilterChange"
          @locate-author="onLocateAuthor"
          @export-image="onExportImage"
          @counting-mode-change="onCountingModeChange"
          @color-mode-change="colorMode = $event"
          @layout-mode-change="onLayoutModeChange"
          @select-cluster="onSelectCluster($event)"
          @clear-clusters="onClearClusters"
          @recalculate="onRecalculate"
          @reset-analysis="onResetAnalysis"
        />
      </aside>

      <!-- Drawer handle (inside wrapper to be relative to it, but outside aside to avoid overflow clipping) -->
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
      <NetworkGraph
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

    <!-- Detail panel -->
    <AuthorDetailPanel
      :author="selectedAuthor"
      :graph="graph"
      @close="onNodeClick(null)"
      @navigate="onNavigateToAuthor"
    />
  </div>
</template>

<style scoped>
.coauthor-layout {
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
</style>
