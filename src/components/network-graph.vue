<template>
  <div class="relative w-full h-full bg-slate-50/50 overflow-hidden">
    <!-- Sigma container -->
    <div ref="sigmaContainer" class="w-full h-full" />

    <!-- Loading / error / empty overlay -->
    <GraphStatusOverlay
      :loading="loading"
      :is-layouting="isLayouting"
      :error="error"
      :empty="!hasGraph"
      loading-label="Loading network…"
      empty-icon="hub"
      empty-text="No network data. Import articles first."
      @retry="$emit('retry')"
    />

    <!-- Hover tooltip -->
    <div
      v-if="hoveredNode"
      class="absolute z-30 pointer-events-none bg-white border border-slate-200 rounded-lg shadow-lg px-3 py-2 text-xs max-w-[220px]"
      :style="tooltipPosition"
    >
      <p class="font-semibold text-slate-800 truncate">{{ hoveredNode.label }}</p>
      <div class="flex gap-3 mt-1 text-slate-500">
        <span>{{ hoveredNode.weight }} papers</span>
        <span v-if="hoveredNode.totalCitations">{{ hoveredNode.totalCitations }} citations</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { watch } from 'vue';
import type Graph from 'graphology';
import { useNetworkGraph } from '../composables/use-network-graph';
import { clusterColor } from '../types/biblio-network';
import type { CoAuthorNode } from '../types/biblio-network';
import type { NetworkGraphProps } from '../types/network-graph';
import { getTemporalColor } from '@/utils/color';
import GraphStatusOverlay from './graph-status-overlay.vue';

const props = defineProps<NetworkGraphProps>();

const emit = defineEmits<{
  (e: 'node-click', nodeId: string | null): void;
  (e: 'retry'): void;
}>();

const { hoveredNode, hasGraph, tooltipPosition, renderer, locateNode, resetZoom, refresh } =
  useNetworkGraph<CoAuthorNode>(props, {
    rendererOptions: {
      labelRenderSizeThreshold: 1.2,
      defaultEdgeColor: '#e2e8f0',
    },
    mapHoveredNode,
    /* The co-author graph dispatches its own focus > cluster > clear logic per
       prop change, so the shared reapply watchers are disabled and installed
       locally below. */
    installStandardWatchers: false,
    onBeforeInit: clearFocusMode,
    onGraphReady: () => {
      if (props.focusedNodeId) {
        applyFocusMode(props.focusedNodeId);
      }
    },
    applyVisualState: clearFocusMode,
    onNodeClick: (nodeId) => emit('node-click', nodeId),
  });

watch(
  () => props.focusedNodeId,
  (newId) => {
    if (newId) {
      applyFocusMode(newId);
    } else {
      clearFocusMode();
    }
  }
);

watch(
  () => props.colorMode,
  () => {
    if (props.focusedNodeId) {
      applyFocusMode(props.focusedNodeId);
    } else if (props.selectedClusters.length > 0) {
      applyClusterHighlight(props.selectedClusters);
    } else {
      clearFocusMode();
    }
  }
);

watch(
  () => props.selectedClusters,
  (clusters) => {
    if (props.focusedNodeId) {
      // Focus mode takes priority; re-apply it
      applyFocusMode(props.focusedNodeId);
    } else if (clusters.length > 0) {
      applyClusterHighlight(clusters);
    } else {
      clearFocusMode();
    }
  },
  { deep: true }
);

watch(
  () => props.recalculateTrigger,
  () => {
    if (props.graph) {
      if (props.focusedNodeId) {
        applyFocusMode(props.focusedNodeId);
      } else if (props.selectedClusters.length > 0) {
        applyClusterHighlight(props.selectedClusters);
      } else {
        clearFocusMode();
      }
    }
  }
);

function getNodeColor(nodeId: string): string {
  if (!props.graph || !props.graph.hasNode(nodeId)) return '#94a3b8';
  if (props.colorMode === 'temporal') {
    const avgYear = props.graph.getNodeAttribute(nodeId, 'avgYear');
    return getTemporalColor(avgYear, props.minYear, props.maxYear);
  } else {
    const cluster = props.graph.getNodeAttribute(nodeId, 'cluster') ?? 0;
    return clusterColor(cluster);
  }
}

function mapHoveredNode(node: string, attrs: ReturnType<Graph['getNodeAttributes']>): CoAuthorNode {
  return {
    id: node,
    label: attrs.label ?? node,
    weight: attrs.weight ?? 0,
    totalCitations: attrs.totalCitations ?? 0,
    avgYear: attrs.avgYear ?? null,
    estimatedHIndex: attrs.estimatedHIndex ?? null,
    cluster: attrs.cluster ?? null,
    color: getNodeColor(node),
  };
}

defineExpose({
  locateNode,
  resetZoom,
  refresh,
  renderer,
});

function applyClusterHighlight(clusterIds: number[]) {
  if (!props.graph) return;
  const g = props.graph;
  const clusterSet = new Set(clusterIds);

  g.forEachNode((n) => {
    const cluster = g.getNodeAttribute(n, 'cluster') as number | null;
    const isInCluster = cluster !== null && clusterSet.has(cluster);
    const baseColor = getNodeColor(n);
    g.setNodeAttribute(n, 'color', isInCluster ? baseColor : `${baseColor}26`);
    const origSize = g.getNodeAttribute(n, 'size') ?? 5;
    g.setNodeAttribute(n, 'size', isInCluster ? origSize : origSize * 0.6);
  });

  g.forEachEdge((_edge, _attrs, source, target) => {
    const sCluster = g.getNodeAttribute(source, 'cluster') as number | null;
    const tCluster = g.getNodeAttribute(target, 'cluster') as number | null;
    const bothInCluster =
      sCluster !== null &&
      tCluster !== null &&
      clusterSet.has(sCluster) &&
      clusterSet.has(tCluster);
    g.setEdgeAttribute(_edge as string, 'color', bothInCluster ? '#94a3b8' : '#f1f5f9');
  });
}

function applyFocusMode(nodeId: string) {
  if (!props.graph || !renderer.value) return;
  const g = props.graph;
  if (!g.hasNode(nodeId)) return;
  const neighbors = new Set(g.neighbors(nodeId));
  neighbors.add(nodeId);

  g.forEachNode((n) => {
    const isNeighbor = neighbors.has(n);
    const baseColor = getNodeColor(n);
    g.setNodeAttribute(n, 'color', isNeighbor ? baseColor : `${baseColor}26`);
    const origSize = g.getNodeAttribute(n, 'size') ?? 5;
    g.setNodeAttribute(n, 'size', isNeighbor ? origSize : origSize * 0.6);
  });

  g.forEachEdge((_edge, _attrs, source, target) => {
    const isConnected = neighbors.has(source) && neighbors.has(target);
    g.setEdgeAttribute(_edge as string, 'color', isConnected ? '#94a3b8' : '#f1f5f9');
  });
}

function clearFocusMode() {
  if (!props.graph) return;
  const g = props.graph;
  const weights: number[] = [];
  g.forEachNode((n) => weights.push(g.getNodeAttribute(n, 'weight') ?? 1));
  const minW = Math.min(...weights, 1);
  const maxW = Math.max(...weights, 1);

  g.forEachNode((n) => {
    g.setNodeAttribute(n, 'color', getNodeColor(n));
    const weight = g.getNodeAttribute(n, 'weight') ?? 1;
    const size = minW === maxW ? 10 : 3 + ((weight - minW) / (maxW - minW)) * 17;
    g.setNodeAttribute(n, 'size', size);
  });

  g.forEachEdge((e) => {
    g.setEdgeAttribute(e as string, 'color', '#e2e8f0');
  });
}
</script>
