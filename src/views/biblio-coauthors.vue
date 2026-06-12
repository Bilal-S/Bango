<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import NetworkGraph from '../components/network-graph.vue';
import NetworkControls from '../components/network-controls.vue';
import AuthorDetailPanel from '../components/author-detail-panel.vue';
import { useCoAuthorNetwork } from '../composables/use-coauthor-network';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import type { CoAuthorNode } from '../types/biblio-network';

const { graph, loading, error, nodeCount, edgeCount, countingMode, fetchNetwork, setCountingMode } =
  useCoAuthorNetwork();

const { isLayouting, applyLayout, runForceAtlas2Async } = useNetworkLayout();
const { locateNode, resetZoom, exportImage, applyGraphFilters } = useSigmaRenderer();

const selectedAuthor = ref<CoAuthorNode | null>(null);
const focusedNodeId = ref<string | null>(null);
const visibleNodeCount = ref(0);
const visibleEdgeCount = ref(0);

const colorMode = ref<'cluster' | 'temporal'>('cluster');

/** Derive cluster count from graph node attributes */
const clusterCount = computed(() => {
  if (!graph.value) return 0;
  const clusters = new Set<number>();
  graph.value.forEachNode((node) => {
    const c = graph.value!.getNodeAttribute(node, 'cluster') as number | null;
    if (c !== null && c !== undefined) clusters.add(c);
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

onMounted(async () => {
  await fetchNetwork();
  if (graph.value) {
    await applyLayout(graph.value);
    // Initialize visible counts
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
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

function onFilterChange(filters: { minPapers: number; minLinkStrength: number; search: string }) {
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

function onResetZoom() {
  resetZoom();
}

function onExportImage() {
  const dataUrl = exportImage();
  if (dataUrl) {
    const link = document.createElement('a');
    link.download = 'coauthor-network.png';
    link.href = dataUrl;
    link.click();
  }
}

async function onCountingModeChange(mode: 'full' | 'fractional') {
  const changed = setCountingMode(mode);
  if (changed && graph.value) {
    // Quick layout polish after weight change
    await runForceAtlas2Async(graph.value, 30);
  }
}
</script>

<template>
  <div class="coauthor-layout">
    <!-- Controls sidebar -->
    <aside class="w-64 shrink-0 p-4 overflow-y-auto border-r border-slate-100 bg-slate-50/30">
      <NetworkControls
        :total-nodes="stats.totalAuthors"
        :total-edges="stats.totalEdges"
        :visible-nodes="stats.visibleAuthors"
        :visible-edges="stats.visibleEdges"
        :cluster-count="stats.clusterCount"
        :author-names="authorNames"
        :counting-mode="countingMode"
        :color-mode="colorMode"
        :min-year="yearRange.min"
        :max-year="yearRange.max"
        @filter-change="onFilterChange"
        @locate-author="onLocateAuthor"
        @reset-zoom="onResetZoom"
        @export-image="onExportImage"
        @counting-mode-change="onCountingModeChange"
        @color-mode-change="colorMode = $event"
      />
    </aside>

    <!-- Graph canvas -->
    <main class="flex-1 relative">
      <NetworkGraph
        :graph="graph"
        :loading="loading"
        :is-layouting="isLayouting"
        :error="error"
        :focused-node-id="focusedNodeId"
        :color-mode="colorMode"
        :min-year="yearRange.min"
        :max-year="yearRange.max"
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
}
</style>
