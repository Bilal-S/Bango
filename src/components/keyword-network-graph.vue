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
      loading-label="Loading keyword network…"
      empty-icon="mediation"
      empty-text="No keyword data matched. Try adjusting sources/thresholds or normalize terms."
      @retry="$emit('retry')"
    />

    <!-- Hover tooltip -->
    <div
      v-if="hoveredNode"
      class="absolute z-30 pointer-events-none bg-white border border-slate-200 rounded-lg shadow-lg px-3 py-2 text-xs max-w-[260px]"
      :style="tooltipPosition"
    >
      <p class="font-semibold text-slate-800 text-sm mb-1">{{ hoveredNode.label }}</p>

      <div class="space-y-1 text-slate-500">
        <div class="flex justify-between gap-4">
          <span>Occurrences:</span>
          <span class="font-medium text-slate-700">{{ hoveredNode.weight }} docs</span>
        </div>
        <div class="flex justify-between gap-4">
          <span>Source:</span>
          <span class="font-medium text-slate-700 capitalize">{{ hoveredNode.source }}</span>
        </div>
        <div v-if="hoveredAvgPerYear !== null" class="flex justify-between gap-4">
          <span>Average/Year:</span>
          <span class="font-medium text-slate-700">{{ hoveredAvgPerYear.toFixed(1) }} /yr</span>
        </div>
        <div
          v-if="hoveredNode.rawTerms && hoveredNode.rawTerms.length > 1"
          class="mt-1.5 pt-1 border-t border-slate-100"
        >
          <p class="text-[10px] text-slate-400 font-semibold mb-0.5">Raw Terms:</p>
          <p class="text-[10px] text-slate-600 leading-tight">
            {{ hoveredNode.rawTerms.join(', ') }}
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type Graph from 'graphology';
import { useNetworkGraph } from '../composables/use-network-graph';
import { applyFocusClusterVisualState } from '../utils/network-visual-state';
import { clusterColor } from '../types/biblio-network';
import type { KeywordNode } from '../types/biblio-keyword';
import type { NetworkGraphProps } from '../types/network-graph';
import { getTemporalColor } from '../utils/color';
import { avgPerYear } from '../utils/formatters';
import GraphStatusOverlay from './graph-status-overlay.vue';

const props = defineProps<NetworkGraphProps>();

const emit = defineEmits<{
  (e: 'node-click', nodeId: string | null): void;
  (e: 'retry'): void;
}>();

const { hoveredNode, hasGraph, tooltipPosition, renderer, locateNode, resetZoom, refresh } =
  useNetworkGraph<KeywordNode>(props, {
    rendererOptions: {
      labelRenderSizeThreshold: 1.0,
      defaultEdgeColor: '#cbd5e1',
      renderEdgeLabels: false,
    },
    mapHoveredNode,
    applyVisualState,
    onNodeClick: (nodeId) => emit('node-click', nodeId),
  });

/** Average occurrences per year for the hovered node (null when no year data). */
const hoveredAvgPerYear = computed(() => avgPerYear(hoveredNode.value?.yearCounts));

/** Node sizing: weight-scaled 5-25 (flat range: 12). */
function getNodeSize(node: string, minW: number, maxW: number): number {
  const g = props.graph!;
  const weight = (g.getNodeAttribute(node, 'weight') as number) ?? 1;
  return minW === maxW ? 12 : 5 + ((weight - minW) / (maxW - minW)) * 20;
}

/** Shared focus/cluster dimming pass; refreshes the renderer when done. */
function applyVisualState() {
  if (!props.graph) return;
  applyFocusClusterVisualState(
    props.graph,
    {
      focusedNodeId: props.focusedNodeId,
      selectedClusters: props.selectedClusters,
    },
    { getNodeColor, getNodeSize }
  );
  renderer.value?.refresh();
}

function getNodeColor(nodeId: string): string {
  if (!props.graph || !props.graph.hasNode(nodeId)) return '#94a3b8';
  if (props.colorMode === 'temporal') {
    const year = props.graph.getNodeAttribute(nodeId, 'avgYear');
    return getTemporalColor(year, props.minYear, props.maxYear);
  } else {
    const cluster = props.graph.getNodeAttribute(nodeId, 'cluster') ?? 0;
    return clusterColor(cluster);
  }
}

function mapHoveredNode(node: string, attrs: ReturnType<Graph['getNodeAttributes']>): KeywordNode {
  return {
    id: node,
    label: attrs.label ?? node,
    weight: attrs.weight ?? 0,
    source: attrs.source ?? '',
    avgYear: attrs.avgYear ?? null,
    yearCounts: attrs.yearCounts ?? [],
    rawTerms: attrs.rawTerms ?? [],
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
