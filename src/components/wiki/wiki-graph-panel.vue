<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import Graph from 'graphology';
import Sigma from 'sigma';
import { NodeCircleProgram, createEdgeArrowProgram } from 'sigma/rendering';
import forceAtlas2 from 'graphology-layout-forceatlas2';
import { useWiki } from '@/composables/use-wiki';
import type { WikiGraph } from '@/types/wiki';

const emit = defineEmits<{
  selectPage: [slug: string];
}>();

const { getGraph } = useWiki();

const containerRef = ref<HTMLDivElement | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
const graph = ref<WikiGraph | null>(null);
const stats = ref({ nodes: 0, edges: 0, orphans: 0 });

let sigma: Sigma | null = null;

/** Color map for page types. */
const typeColors: Record<string, string> = {
  concept: '#6366f1', // indigo
  author: '#22c55e', // green
  method: '#f97316', // orange
  synthesis: '#a855f7', // purple
  source: '#64748b', // slate
};

/** Build and render the graph. */
async function loadAndRender(): Promise<void> {
  loading.value = true;
  error.value = null;
  try {
    graph.value = await getGraph();
    stats.value = {
      nodes: graph.value.nodes.length,
      edges: graph.value.edges.length,
      orphans: graph.value.orphanCount,
    };
    if (graph.value.nodes.length === 0) {
      return;
    }
    render();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

/** Build a graphology graph from WikiGraph data and render with sigma. */
function render(): void {
  if (!containerRef.value || !graph.value) return;

  // Destroy any existing renderer.
  if (sigma) {
    sigma.kill();
    sigma = null;
  }

  const g = new Graph({ multi: false, type: 'directed' });

  // Add nodes.
  for (const node of graph.value.nodes) {
    const color = typeColors[node.pageType] ?? '#94a3b8';
    const size = 5 + Math.min(Math.max(node.inbound, node.outbound), 10);
    g.addNode(node.slug, {
      label: node.title,
      size,
      color,
      x: Math.random() * 100,
      y: Math.random() * 100,
      pageType: node.pageType,
    });
  }

  // Add edges (only between known nodes — broken links are skipped).
  const knownSlugs = new Set(graph.value.nodes.map((n) => n.slug));
  for (const edge of graph.value.edges) {
    if (knownSlugs.has(edge.source) && knownSlugs.has(edge.target)) {
      g.addEdge(edge.source, edge.target, {
        size: 1,
        color: '#cbd5e1',
        type: 'arrow',
      });
    }
  }

  // Apply ForceAtlas2 layout.
  forceAtlas2.assign(g, {
    iterations: 80,
    settings: {
      linLogMode: true,
      adjustSizes: true,
      gravity: 2,
      scalingRatio: 3,
      barnesHutOptimize: g.order > 200,
    },
  });

  // Create sigma renderer.
  sigma = new Sigma(g, containerRef.value, {
    renderEdgeLabels: false,
    defaultNodeColor: '#94a3b8',
    defaultEdgeColor: '#cbd5e1',
    labelDensity: 0.3,
    labelGridCellSize: 60,
    labelRenderedSizeThreshold: 6,
    nodeProgramClasses: {
      circle: NodeCircleProgram,
    },
    edgeProgramClasses: {
      arrow: createEdgeArrowProgram(),
    },
  });

  // Click handler: select the clicked node.
  sigma.on('clickNode', ({ node }) => {
    emit('selectPage', node);
  });
}

/** Legend items for the type colors. */
const legendItems = [
  { label: 'Concepts', color: typeColors.concept! },
  { label: 'Authors', color: typeColors.author! },
  { label: 'Methods', color: typeColors.method! },
  { label: 'Synthesis', color: typeColors.synthesis! },
];

onMounted(loadAndRender);
onUnmounted(() => {
  if (sigma) {
    sigma.kill();
    sigma = null;
  }
});

// Expose a refresh method for the parent to call after ingest.
defineExpose({ refresh: loadAndRender });
</script>

<template>
  <div class="wiki-graph-panel relative w-full h-full bg-slate-50/50 overflow-hidden">
    <!-- Sigma container -->
    <div ref="containerRef" class="w-full h-full" />

    <!-- Loading overlay -->
    <div
      v-if="loading"
      class="absolute inset-0 z-20 flex items-center justify-center bg-white/60 backdrop-blur-sm"
    >
      <div class="flex items-center gap-3 text-slate-600">
        <span class="material-symbols-outlined text-xl animate-spin">progress_activity</span>
        <span class="text-sm font-medium">Building graph...</span>
      </div>
    </div>

    <!-- Error overlay -->
    <div v-else-if="error" class="absolute inset-0 z-20 flex items-center justify-center">
      <div class="text-center p-6 max-w-sm">
        <span class="material-symbols-outlined text-3xl text-red-400 mb-2 block">error</span>
        <p class="text-sm text-red-600">{{ error }}</p>
        <button
          class="mt-3 px-3 py-1.5 text-xs font-semibold text-white bg-indigo-600 hover:bg-indigo-700 rounded-lg cursor-pointer transition-colors"
          @click="loadAndRender"
        >
          Retry
        </button>
      </div>
    </div>

    <!-- Empty state -->
    <div
      v-else-if="graph && graph.nodes.length === 0"
      class="absolute inset-0 z-20 flex items-center justify-center"
    >
      <div class="text-center text-slate-400">
        <span class="material-symbols-outlined text-4xl mb-2 block">hub</span>
        <p class="text-sm">No pages to graph. Ingest sources first.</p>
      </div>
    </div>

    <!-- Stats + Legend overlay (top-right) -->
    <div
      v-if="graph && graph.nodes.length > 0"
      class="absolute top-3 right-3 z-10 bg-white/90 backdrop-blur-sm rounded-lg border border-slate-200 shadow-sm p-3 text-xs space-y-2"
    >
      <div class="flex items-center gap-3 font-mono text-slate-600">
        <span>{{ stats.nodes }} nodes</span>
        <span>{{ stats.edges }} edges</span>
        <span v-if="stats.orphans > 0" class="text-amber-600">{{ stats.orphans }} orphans</span>
      </div>
      <div class="flex items-center gap-2 flex-wrap">
        <div v-for="item in legendItems" :key="item.label" class="flex items-center gap-1">
          <span
            class="inline-block w-2.5 h-2.5 rounded-full"
            :style="{ backgroundColor: item.color }"
          ></span>
          <span class="text-slate-500">{{ item.label }}</span>
        </div>
      </div>
    </div>

    <!-- Hint (bottom-left) -->
    <div
      v-if="graph && graph.nodes.length > 0"
      class="absolute bottom-3 left-3 z-10 text-[10px] text-slate-400 bg-white/80 px-2 py-1 rounded"
    >
      Click a node to open its page. Drag to pan. Scroll to zoom.
    </div>
  </div>
</template>
