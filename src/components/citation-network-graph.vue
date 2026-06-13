<template>
  <div ref="containerRef" class="relative w-full h-full bg-slate-50/50 overflow-hidden">
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
const hoveredNode = ref<CitationNode | null>(null);
const tooltipX = ref(0);
const tooltipY = ref(0);

const { renderer, initRenderer, destroyRenderer } = useSigmaRenderer();

const hasGraph = computed(() => (props.graph?.order ?? 0) > 0);

const tooltipPosition = computed(() => ({
  left: `${tooltipX.value + 12}px`,
  top: `${tooltipY.value - 8}px`,
}));

watch(
  () => props.graph,
  (g) => {
    if (!g || !containerRef.value) return;
    clearFocusMode();
    requestAnimationFrame(() => {
      if (!containerRef.value || !g) return;
      initRenderer(containerRef.value, g, {
        labelRenderSizeThreshold: 1.2,
        defaultEdgeColor: '#cbd5e1',
        renderEdgeLabels: false,
      });
      bindSigmaEvents();
      if (props.focusedNodeId) {
        applyFocusMode(props.focusedNodeId);
      }
    });
  }
);

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

  // For directed graphs, include both in-neighbors (citing) and out-neighbors (cited)
  const neighbors = new Set<string>([...g.inNeighbors(nodeId), ...g.outNeighbors(nodeId)]);
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

  // Recalculate sizes based on numCited (same as buildGraph).
  // Only consider matched (real article) nodes for the scale range so that
  // unmatched leaves (always 0 citations) don't compress the scale.
  const citedValues: number[] = [];
  g.forEachNode((n) => {
    if (g.getNodeAttribute(n, 'unmatched') !== true) {
      citedValues.push(g.getNodeAttribute(n, 'numCited') ?? 0);
    }
  });
  const minCited = Math.min(...citedValues, 0);
  const maxCited = Math.max(...citedValues, 1);

  g.forEachNode((n) => {
    g.setNodeAttribute(n, 'color', getNodeColor(n));
    const isUnmatched = g.getNodeAttribute(n, 'unmatched') === true;
    if (isUnmatched) {
      // Unmatched leaves are always small; don't scale with citations.
      g.setNodeAttribute(n, 'size', 3);
      return;
    }
    const numCited = g.getNodeAttribute(n, 'numCited') ?? 0;
    const size =
      minCited === maxCited ? 10 : 4 + ((numCited - minCited) / (maxCited - minCited)) * 18;
    g.setNodeAttribute(n, 'size', size);
  });

  g.forEachEdge((e) => {
    const isUnmatched = g.getEdgeAttribute(e as string, 'unmatched') === true;
    g.setEdgeAttribute(
      e as string,
      'color',
      isUnmatched ? '#e2e8f0' : '#cbd5e1' // slate-200 : slate-300
    );
  });
}

onUnmounted(() => {
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
