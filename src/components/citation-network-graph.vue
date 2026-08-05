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
          isLayouting ? 'Computing layout…' : 'Loading citation network…'
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
        <span class="material-symbols-outlined text-4xl mb-2 block">account_tree</span>
        <p class="text-sm">No citation data. Import articles with references first.</p>
      </div>
    </div>

    <!-- Hover tooltip -->
    <div
      v-if="hoveredNode"
      class="absolute z-30 pointer-events-none bg-white border border-slate-200 rounded-lg shadow-lg px-3 py-2 text-xs max-w-[240px]"
      :style="tooltipPosition"
    >
      <p class="font-semibold text-slate-800">{{ hoveredNode.label }}</p>
      <p v-if="hoveredNode.title" class="text-slate-500 mt-0.5 line-clamp-2">
        {{ hoveredNode.title }}
      </p>
      <div class="flex gap-3 mt-1 text-slate-500">
        <span class="flex items-center gap-0.5">
          <span class="material-symbols-outlined text-[10px]">arrow_downward</span>
          {{ hoveredNode.numCited }} cited
        </span>
        <span class="flex items-center gap-0.5">
          <span class="material-symbols-outlined text-[10px]">arrow_upward</span>
          {{ hoveredNode.numReferences }} refs
        </span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue';
import type Graph from 'graphology';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { citationClusterColor } from '../types/biblio-citation';
import type { CitationNode } from '../types/biblio-citation';
import { getTemporalColor } from '@/utils/color';
import { computeAncestry, computeProgeny } from '../utils/citation-analysis';

/** Isolation mode: focus on a node's ancestry (papers it cites) or progeny (papers citing it). */
export type IsolationDirection = 'ancestry' | 'progeny';

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
  /** When set, non-isolated nodes are dimmed. Takes visual precedence over focus/cluster. */
  isolationMode: { nodeId: string; direction: IsolationDirection; label?: string } | null;
  /** Phase 3 - Main Path (SPC): node IDs on the main path backbone. */
  mainPathNodes: Set<string>;
  /** Phase 3 - Main Path (SPC): edge IDs on the main path. */
  mainPathEdges: Set<string>;
  /** Phase 3 - Main Path (SPC): master toggle for the highlight. */
  showMainPath: boolean;
}>();

const emit = defineEmits<{
  (e: 'node-click', nodeId: string | null): void;
  (e: 'retry'): void;
}>();

const containerRef = ref<HTMLElement>();
const hoveredNode = ref<CitationNode | null>(null);
const tooltipX = ref(0);
const tooltipY = ref(0);

/* Guard against async callbacks (rAF, worker results) firing after unmount.
   Without this, a pending rAF can call initRenderer() on a detached container
   during route transitions, causing crashes. */
let isUnmounted = false;
let pendingFrame: number | null = null;

const { renderer, initRenderer, destroyRenderer, locateNode, resetZoom, refresh } =
  useSigmaRenderer();

const hasGraph = computed(() => (props.graph?.order ?? 0) > 0);

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
      // Abort if the component was unmounted while we waited for the frame.
      // This prevents mounting a Sigma renderer onto a detached DOM node.
      if (isUnmounted || !containerRef.value || !g) return;
      initRenderer(containerRef.value, g, {
        labelRenderSizeThreshold: 1.2,
        defaultEdgeColor: '#cbd5e1',
        renderEdgeLabels: false,
        // Enlarge arrowheads so citation direction is clearly visible.
        // Sigma defaults are length 2.5 / wideness 2.
        edgeArrowSize: { length: 4, wideness: 3 },
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

/** Centralized visual state dispatch. Isolation > focus > cluster highlight.
 *  None active → default full-brightness restored. */
function applyVisualState() {
  if (!props.graph) return;
  const g = props.graph;

  const isIsolationActive = !!props.isolationMode;
  const isMainPathActive = props.showMainPath && props.mainPathNodes.size > 0;
  const isFocusActive = !!props.focusedNodeId;
  const isClusterActive = props.selectedClusters.length > 0;

  // Precompute sets for quick lookup
  const isolationSet = new Set<string>();
  if (props.isolationMode && g.hasNode(props.isolationMode.nodeId)) {
    const focusId = props.isolationMode.nodeId;
    isolationSet.add(focusId);
    if (props.isolationMode.direction === 'ancestry') {
      for (const n of computeAncestry(g, focusId)) isolationSet.add(n);
    } else {
      for (const n of computeProgeny(g, focusId)) isolationSet.add(n);
    }
  }

  let focusNeighborsSet = new Set<string>();
  if (props.focusedNodeId && g.hasNode(props.focusedNodeId)) {
    const focusId = props.focusedNodeId;
    focusNeighborsSet = new Set([...g.inNeighbors(focusId), ...g.outNeighbors(focusId), focusId]);
  }

  const selectedClustersSet = new Set(props.selectedClusters);

  // Pre-calculate base node size scaling bounds
  const citedValues: number[] = [];
  g.forEachNode((n) => {
    if (g.getNodeAttribute(n, 'unmatched') !== true) {
      citedValues.push(g.getNodeAttribute(n, 'numCited') ?? 0);
    }
  });
  const minCited = Math.min(...citedValues, 0);
  const maxCited = Math.max(...citedValues, 1);

  // Apply composed visual attributes to nodes
  g.forEachNode((n) => {
    const isUnmatched = g.getNodeAttribute(n, 'unmatched') === true;
    const baseColor = getNodeColor(n);

    // Base size computation
    let baseSize = 3;
    if (!isUnmatched) {
      const numCited = g.getNodeAttribute(n, 'numCited') ?? 0;
      baseSize =
        minCited === maxCited ? 10 : 4 + ((numCited - minCited) / (maxCited - minCited)) * 18;
    }

    const isIsolatedDimmed = isIsolationActive && !isolationSet.has(n);
    const isOnMainPathDimmed = isMainPathActive && !props.mainPathNodes.has(n);
    const isFocusedDimmed = isFocusActive && !focusNeighborsSet.has(n);
    const isInClusterDimmed =
      isClusterActive &&
      (g.getNodeAttribute(n, 'cluster') === null ||
        !selectedClustersSet.has(g.getNodeAttribute(n, 'cluster') as number));

    const isDimmed = isIsolatedDimmed || isOnMainPathDimmed || isFocusedDimmed || isInClusterDimmed;

    if (isDimmed) {
      g.setNodeAttribute(n, 'color', `${baseColor}26`); // 15% opacity
      g.setNodeAttribute(n, 'size', baseSize * 0.6);
    } else {
      g.setNodeAttribute(n, 'color', baseColor);
      const sizeMultiplier = isMainPathActive && props.mainPathNodes.has(n) ? 1.15 : 1.0;
      g.setNodeAttribute(n, 'size', baseSize * sizeMultiplier);
    }

    g.setNodeAttribute(n, 'type', isUnmatched ? 'circle' : 'included');
  });

  // Apply composed visual attributes to edges
  g.forEachEdge((edge, attrs, source, target) => {
    const isUnmatchedEdge = attrs.unmatched === true;

    const isIsolatedEdgeDimmed =
      isIsolationActive && (!isolationSet.has(source) || !isolationSet.has(target));
    const isOnMainPathEdgeDimmed = isMainPathActive && !props.mainPathEdges.has(edge as string);
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

    const isDimmedEdge =
      isIsolatedEdgeDimmed || isOnMainPathEdgeDimmed || isFocusedEdgeDimmed || isClusterEdgeDimmed;

    if (isDimmedEdge) {
      g.setEdgeAttribute(edge as string, 'color', '#f1f5f9'); // highly dimmed (slate-100)
    } else {
      if (isMainPathActive && props.mainPathEdges.has(edge as string)) {
        g.setEdgeAttribute(edge as string, 'color', '#f5473a'); // main path highlight color
      } else if (isIsolationActive) {
        g.setEdgeAttribute(edge as string, 'color', '#6366f1'); // isolation direction connection
      } else if (isFocusActive || isClusterActive) {
        g.setEdgeAttribute(edge as string, 'color', '#94a3b8'); // highlighted neighbor connection
      } else {
        g.setEdgeAttribute(edge as string, 'color', isUnmatchedEdge ? '#e2e8f0' : '#cbd5e1');
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

watch(
  () => props.isolationMode,
  () => applyVisualState(),
  { deep: true }
);

/**
 * Phase 3 - Main Path (SPC): re-apply visual state when the toggle or the
 * computed node/edge sets change.
 */
watch(
  () => [props.showMainPath, props.mainPathNodes, props.mainPathEdges] as const,
  () => applyVisualState(),
  { deep: true }
);

function getNodeColor(nodeId: string): string {
  if (!props.graph || !props.graph.hasNode(nodeId)) return '#94a3b8';
  // Unmatched reference leaves are always muted grey, regardless of color mode,
  // so they read as supplementary context rather than primary nodes.
  const isUnmatched = props.graph.getNodeAttribute(nodeId, 'unmatched') === true;
  if (isUnmatched) return '#94a3b8'; // slate-400
  if (props.colorMode === 'temporal') {
    const year = props.graph.getNodeAttribute(nodeId, 'year');
    return getTemporalColor(year, props.minYear, props.maxYear);
  } else {
    const cluster = props.graph.getNodeAttribute(nodeId, 'cluster') ?? 0;
    return citationClusterColor(cluster);
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
      title: attrs.title ?? '',
      authors: attrs.authors ?? '',
      year: attrs.year ?? null,
      journal: attrs.journal ?? null,
      numCited: attrs.numCited ?? 0,
      numReferences: attrs.numReferences ?? 0,
      abstract: attrs.abstract ?? '',
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

<style scoped>
.line-clamp-2 {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
</style>
