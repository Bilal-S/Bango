<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import NetworkGraph from '../components/network-graph.vue';
import NetworkControls from '../components/network-controls.vue';
import AuthorDetailPanel from '../components/author-detail-panel.vue';
import { useCoAuthorNetwork } from '../composables/use-coauthor-network';
import { useNetworkView } from '../composables/use-network-view';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import type { NetworkExportFormat } from '../utils/network-export';
import type { CoAuthorNode } from '../types/biblio-network';

const { graph, loading, error, nodeCount, edgeCount, countingMode, fetchNetwork, setCountingMode } =
  useCoAuthorNetwork();

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
  exportPrefix: 'coauthor-network',
  yearAttribute: 'avgYear',
});

const { runForceAtlas2Async } = useNetworkLayout();
const { applyGraphFilters } = useSigmaRenderer();

const selectedAuthor = ref<CoAuthorNode | null>(null);

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
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
});

function onNodeClick(nodeId: string | null) {
  focusNode(nodeId);
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
  graphRef.value?.locateNode(nodeId);
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

async function onExportImage(format: NetworkExportFormat) {
  const renderer = (graphRef.value as { renderer?: unknown } | null)?.renderer;
  await exportImage(format, (renderer as Parameters<typeof exportImage>[1]) ?? null);
}

async function onCountingModeChange(mode: 'full' | 'fractional') {
  const changed = setCountingMode(mode);
  if (changed && graph.value) {
    await runForceAtlas2Async(graph.value, 30, layoutMode.value);
  }
}

async function onResetAnalysis() {
  resetViewState();
  selectedAuthor.value = null;
  const changed = setCountingMode('full');
  if (changed && graph.value) {
    await runForceAtlas2Async(graph.value, 30, layoutMode.value);
  }
  await onRecalculate();
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
          @locate-author="(name: string) => locateByLabel(name)"
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
</style>
