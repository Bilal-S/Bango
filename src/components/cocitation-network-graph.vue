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
      loading-label="Loading co-citation network…"
      empty-icon="hub"
      empty-title="No co-citation data"
      empty-hint="Adjust thresholds, import reference data, or try a different normalization mode."
      @retry="$emit('retry')"
    />

    <!-- Hover tooltip -->
    <div
      v-if="hoveredNode"
      class="absolute z-30 pointer-events-none bg-white border border-slate-200 rounded-lg shadow-lg px-3 py-2 text-xs max-w-[280px]"
      :style="tooltipPosition"
    >
      <p class="font-semibold text-slate-800 text-sm mb-1">{{ hoveredNode.label }}</p>

      <div class="space-y-1 text-slate-500">
        <div v-if="hoveredNode.title" class="text-[11px] text-slate-600 leading-tight">
          {{ hoveredNode.title }}
        </div>
        <div class="flex justify-between gap-4">
          <span>Cited by (in-scope):</span>
          <span class="font-medium text-slate-700">{{ hoveredNode.coCitationCount }} articles</span>
        </div>
        <div class="flex justify-between gap-4">
          <span>Total citations:</span>
          <span class="font-medium text-slate-700">{{ hoveredNode.citationCount }}</span>
        </div>
        <div v-if="hoveredNode.year" class="flex justify-between gap-4">
          <span>Year:</span>
          <span class="font-medium text-slate-700">{{ hoveredNode.year }}</span>
        </div>
        <div v-if="hoveredNode.journal" class="flex justify-between gap-4">
          <span>Journal:</span>
          <span class="font-medium text-slate-700 truncate">{{ hoveredNode.journal }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type Graph from 'graphology';
import { useNetworkGraph } from '../composables/use-network-graph';
import { scaleToRange } from '../composables/use-biblio-network-fetch';
import { applyFocusClusterVisualState } from '../utils/network-visual-state';
import { clusterColor } from '../types/biblio-network';
import type { CocitationNode } from '../types/biblio-cocitation';
import type { NetworkGraphProps } from '../types/network-graph';
import { getTemporalColor } from '../utils/color';
import GraphStatusOverlay from './graph-status-overlay.vue';

const props = defineProps<NetworkGraphProps>();

const emit = defineEmits<{
  (e: 'node-click', nodeId: string | null): void;
  (e: 'retry'): void;
}>();

const { hoveredNode, hasGraph, tooltipPosition, renderer, locateNode, resetZoom, refresh } =
  useNetworkGraph<CocitationNode>(props, {
    rendererOptions: {
      labelRenderSizeThreshold: 1.0,
      defaultEdgeColor: '#cbd5e1',
      renderEdgeLabels: false,
    },
    mapHoveredNode,
    applyVisualState,
    onNodeClick: (nodeId) => emit('node-click', nodeId),
  });

/** Shared focus/cluster pass: stored node sizes + scaled edge thickness. */
function applyVisualState() {
  if (!props.graph) return;
  const g = props.graph;
  applyFocusClusterVisualState(
    g,
    { focusedNodeId: props.focusedNodeId, selectedClusters: props.selectedClusters },
    {
      getNodeColor,
      getNodeSize: (node) => (g.getNodeAttribute(node, 'size') as number) ?? 8,
      computeEdgeSize: (edge, minW, maxW) => {
        const weight = (g.getEdgeAttribute(edge, 'weight') as number) ?? 1;
        return minW === maxW ? 1.5 : scaleToRange(weight, minW, maxW, 0.8, 4);
      },
    }
  );
  renderer.value?.refresh();
}

function getNodeColor(nodeId: string): string {
  if (!props.graph || !props.graph.hasNode(nodeId)) return '#94a3b8';
  if (props.colorMode === 'temporal') {
    const year = props.graph.getNodeAttribute(nodeId, 'year');
    return getTemporalColor(year, props.minYear, props.maxYear);
  }
  const cluster = props.graph.getNodeAttribute(nodeId, 'cluster') ?? 0;
  return clusterColor(cluster);
}

function mapHoveredNode(
  node: string,
  attrs: ReturnType<Graph['getNodeAttributes']>
): CocitationNode {
  return {
    id: node,
    label: attrs.label ?? node,
    title: attrs.title ?? '',
    authors: attrs.authors ?? '',
    year: attrs.year ?? null,
    journal: attrs.journal ?? null,
    doi: attrs.doi ?? null,
    citationCount: attrs.citationCount ?? 0,
    coCitationCount: attrs.coCitationCount ?? 0,
    matchedArticleId: attrs.matchedArticleId ?? null,
    matchedArticleStatus: attrs.matchedArticleStatus ?? null,
    abstract: attrs.abstract ?? '',
    referenceType: attrs.referenceType ?? null,
  };
}

defineExpose({
  locateNode,
  resetZoom,
  refresh,
  renderer,
});
</script>
