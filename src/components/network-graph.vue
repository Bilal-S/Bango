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
import type Graph from 'graphology';
import { useNetworkGraph } from '../composables/use-network-graph';
import { applyFocusClusterVisualState } from '../utils/network-visual-state';
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
    applyVisualState,
    onNodeClick: (nodeId) => emit('node-click', nodeId),
  });

/* Co-author priority: an active focus ignores the cluster selection (the
   previous local watcher dispatch behaved the same way). The shared
   composable watchers re-apply on focus/color/cluster/recalculate changes. */
function applyVisualState() {
  if (!props.graph) return;
  applyFocusClusterVisualState(
    props.graph,
    {
      focusedNodeId: props.focusedNodeId,
      selectedClusters: props.focusedNodeId ? [] : props.selectedClusters,
    },
    { getNodeColor, getNodeSize: coAuthorNodeSize, idleEdgeColor: '#e2e8f0' }
  );
}

/** Node sizing: weight-scaled 3-20 (flat range: 10). */
function coAuthorNodeSize(node: string, minW: number, maxW: number): number {
  const g = props.graph!;
  const weight = (g.getNodeAttribute(node, 'weight') as number) ?? 1;
  return minW === maxW ? 10 : 3 + ((weight - minW) / (maxW - minW)) * 17;
}

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
</script>
