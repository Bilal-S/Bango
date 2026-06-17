<template>
  <div class="relative w-full h-full bg-slate-50/50 overflow-hidden">
    <!-- Sigma container -->
    <div ref="containerRef" class="w-full h-full" />

    <!-- Loading overlay -->
    <div
      v-if="loading || isLayouting"
      class="absolute inset-0 z-20 flex items-center justify-center bg-white/60 backdrop-blur-sm"
    >
      <div class="flex items-center gap-3 text-slate-600">
        <span class="material-symbols-outlined text-xl animate-spin">progress_activity</span>
        <span class="text-sm font-medium">{{
          isLayouting ? 'Computing layout…' : 'Loading keyword network…'
        }}</span>
      </div>
    </div>

    <!-- Error overlay -->
    <div v-else-if="error" class="absolute inset-0 z-20 flex items-center justify-center">
      <div class="text-center p-6 max-w-sm">
        <span class="material-symbols-outlined text-3xl text-red-400 mb-2 block">error</span>
        <p class="text-sm text-red-600">{{ error }}</p>
        <button
          class="mt-3 px-3 py-1.5 text-xs font-semibold text-white bg-indigo-600 hover:bg-indigo-700 rounded-lg cursor-pointer transition-colors"
          @click="$emit('retry')"
        >
          Retry
        </button>
      </div>
    </div>

    <!-- Empty state -->
    <div v-else-if="!hasGraph" class="absolute inset-0 z-20 flex items-center justify-center">
      <div class="text-center text-slate-400">
        <span class="material-symbols-outlined text-4xl mb-2 block">mediation</span>
        <p class="text-sm">
          No keyword data matched. Try adjusting sources/thresholds or normalize terms.
        </p>
      </div>
    </div>

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
import { ref, computed, watch, onUnmounted } from 'vue';
import type Graph from 'graphology';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { clusterColor } from '../types/biblio-network';
import type { KeywordNode } from '../types/biblio-keyword';
import { getTemporalColor } from '../utils/color';
import { avgPerYear } from '../utils/formatters';

const props = defineProps<{
  graph: Graph | null;
  loading: boolean;
  isLayouting: boolean;
  error: string | null;
  focusedNodeId: string | null;
  selectedClusters: number[];
  colorMode: 'cluster' | 'temporal';
  minYear: number;
  maxYear: number;
  recalculateTrigger: number;
}>();

const emit = defineEmits<{
  (e: 'node-click', nodeId: string | null): void;
  (e: 'retry'): void;
}>();

const containerRef = ref<HTMLElement>();
const hoveredNode = ref<KeywordNode | null>(null);
const tooltipX = ref(0);
const tooltipY = ref(0);

let isUnmounted = false;
let pendingFrame: number | null = null;

const { renderer, initRenderer, destroyRenderer, locateNode, resetZoom, refresh } =
  useSigmaRenderer();

const hasGraph = computed(() => (props.graph?.order ?? 0) > 0);

/** Average occurrences per year for the hovered node (null when no year data). */
const hoveredAvgPerYear = computed(() => avgPerYear(hoveredNode.value?.yearCounts));

const tooltipPosition = computed(() => ({
  left: `${tooltipX.value + 12}px`,
  top: `${tooltipY.value - 8}px`,
}));

watch(
  () => props.graph,
  (g) => {
    if (pendingFrame !== null) {
      cancelAnimationFrame(pendingFrame);
      pendingFrame = null;
    }
    if (!g) {
      destroyRenderer();
      return;
    }
    if (!containerRef.value) return;
    pendingFrame = requestAnimationFrame(() => {
      pendingFrame = null;
      if (isUnmounted || !containerRef.value || !g) return;
      initRenderer(containerRef.value, g, {
        labelRenderSizeThreshold: 1.0,
        defaultEdgeColor: '#cbd5e1',
        renderEdgeLabels: false,
      });
      bindSigmaEvents();
      applyVisualState();
    });
  }
);

watch(
  () => props.focusedNodeId,
  () => {
    applyVisualState();
  }
);

function applyVisualState() {
  if (!props.graph) return;
  const g = props.graph;

  const isFocusActive = !!props.focusedNodeId;
  const isClusterActive = props.selectedClusters.length > 0;

  let focusNeighborsSet = new Set<string>();
  if (props.focusedNodeId && g.hasNode(props.focusedNodeId)) {
    const focusId = props.focusedNodeId;
    focusNeighborsSet = new Set([...g.neighbors(focusId), focusId]);
  }

  const selectedClustersSet = new Set(props.selectedClusters);

  // Pre-calculate weight boundaries
  const weights: number[] = [];
  g.forEachNode((n) => {
    weights.push(g.getNodeAttribute(n, 'weight') ?? 1);
  });
  const minW = Math.min(...weights, 1);
  const maxW = Math.max(...weights, 1);

  // Apply colors and sizes to nodes
  g.forEachNode((n) => {
    const baseColor = getNodeColor(n);
    const weight = g.getNodeAttribute(n, 'weight') ?? 1;

    // Node sizes
    const baseSize = minW === maxW ? 12 : 5 + ((weight - minW) / (maxW - minW)) * 20;

    const isFocusedDimmed = isFocusActive && !focusNeighborsSet.has(n);
    const isInClusterDimmed =
      isClusterActive &&
      (g.getNodeAttribute(n, 'cluster') === null ||
        !selectedClustersSet.has(g.getNodeAttribute(n, 'cluster') as number));

    const isDimmed = isFocusedDimmed || isInClusterDimmed;

    if (isDimmed) {
      g.setNodeAttribute(n, 'color', `${baseColor}26`); // 15% opacity
      g.setNodeAttribute(n, 'size', baseSize * 0.6);
    } else {
      g.setNodeAttribute(n, 'color', baseColor);
      g.setNodeAttribute(n, 'size', baseSize);
    }
  });

  // Apply colors and thickness to edges
  g.forEachEdge((edge, _attrs, source, target) => {
    const isFocusedEdgeDimmed =
      isFocusActive && (!focusNeighborsSet.has(source) || !focusNeighborsSet.has(target));

    let isClusterEdgeDimmed = false;
    if (isClusterActive) {
      const sCluster = g.getNodeAttribute(source, 'cluster') as number | null;
      const tCluster = g.getNodeAttribute(target, 'cluster') as number | null;
      isClusterEdgeDimmed =
        sCluster === null ||
        tCluster === null ||
        !selectedClustersSet.has(sCluster) ||
        !selectedClustersSet.has(tCluster);
    }

    const isDimmedEdge = isFocusedEdgeDimmed || isClusterEdgeDimmed;

    if (isDimmedEdge) {
      g.setEdgeAttribute(edge, 'color', '#f1f5f9');
    } else {
      if (isFocusActive || isClusterActive) {
        g.setEdgeAttribute(edge, 'color', '#94a3b8');
      } else {
        g.setEdgeAttribute(edge, 'color', '#cbd5e1');
      }
    }
  });

  renderer.value?.refresh();
}

watch(
  () => props.colorMode,
  () => applyVisualState()
);

watch(
  () => props.selectedClusters,
  () => applyVisualState(),
  { deep: true }
);

watch(
  () => props.recalculateTrigger,
  () => {
    if (props.graph) applyVisualState();
  }
);

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

function bindSigmaEvents() {
  if (!renderer.value) return;
  const sig = renderer.value;

  sig.on('enterNode', ({ node }) => {
    if (!props.graph) return;
    const attrs = props.graph.getNodeAttributes(node);
    hoveredNode.value = {
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
  });

  sig.on('leaveNode', () => {
    hoveredNode.value = null;
  });

  sig.on('moveBody', (payload) => {
    const mouseEvt = payload.event.original as MouseEvent;
    if (!mouseEvt.x) return;
    const rect = containerRef.value?.getBoundingClientRect();
    if (rect) {
      tooltipX.value = mouseEvt.x - rect.left;
      tooltipY.value = mouseEvt.y - rect.top;
    }
  });

  sig.on('clickNode', ({ node }) => {
    emit('node-click', node);
  });

  sig.on('clickStage', () => {
    emit('node-click', null);
  });
}

defineExpose({
  locateNode,
  resetZoom,
  refresh,
  renderer,
});

onUnmounted(() => {
  isUnmounted = true;
  if (pendingFrame !== null) {
    cancelAnimationFrame(pendingFrame);
    pendingFrame = null;
  }
  destroyRenderer();
});
</script>
