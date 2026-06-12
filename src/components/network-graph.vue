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
          isLayouting ? 'Computing layout…' : 'Loading network…'
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
        <span class="material-symbols-outlined text-4xl mb-2 block">hub</span>
        <p class="text-sm">No network data. Import articles first.</p>
      </div>
    </div>

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
import { ref, computed, watch, onUnmounted } from 'vue';
import type Graph from 'graphology';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { clusterColor } from '../types/biblio-network';
import type { CoAuthorNode } from '../types/biblio-network';

const props = defineProps<{
  graph: Graph | null;
  loading: boolean;
  isLayouting: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  (e: 'node-click', nodeId: string): void;
  (e: 'retry'): void;
}>();

const containerRef = ref<HTMLElement>();
const hoveredNode = ref<CoAuthorNode | null>(null);
const tooltipX = ref(0);
const tooltipY = ref(0);
const focusedNodeId = ref<string | null>(null);

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
    requestAnimationFrame(() => {
      if (!containerRef.value || !g) return;
      initRenderer(containerRef.value, g, {
        labelRenderSizeThreshold: 1.2,
        defaultEdgeColor: '#e2e8f0',
      });
      bindSigmaEvents();
    });
  }
);

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
      totalCitations: attrs.totalCitations ?? 0,
      avgYear: attrs.avgYear ?? null,
      estimatedHIndex: attrs.estimatedHIndex ?? null,
      cluster: attrs.cluster ?? null,
      color: attrs.color,
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
    focusedNodeId.value = node;
    applyFocusMode(node);
    emit('node-click', node);
  });

  sig.on('clickStage', () => {
    clearFocusMode();
    focusedNodeId.value = null;
  });
}

function applyFocusMode(nodeId: string) {
  if (!props.graph || !renderer.value) return;
  const g = props.graph;
  const neighbors = new Set(g.neighbors(nodeId));
  neighbors.add(nodeId);

  g.forEachNode((n) => {
    const isNeighbor = neighbors.has(n);
    const originalColor = g.getNodeAttribute(n, 'color') ?? '#94a3b8';
    g.setNodeAttribute(n, 'color', isNeighbor ? originalColor : `${originalColor}26`);
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
    const cluster = g.getNodeAttribute(n, 'cluster') ?? 0;
    g.setNodeAttribute(n, 'color', clusterColor(cluster));
    const weight = g.getNodeAttribute(n, 'weight') ?? 1;
    const size = minW === maxW ? 10 : 3 + ((weight - minW) / (maxW - minW)) * 17;
    g.setNodeAttribute(n, 'size', size);
  });

  g.forEachEdge((e) => {
    g.setEdgeAttribute(e as string, 'color', '#e2e8f0');
  });
}

onUnmounted(() => {
  destroyRenderer();
});
</script>
