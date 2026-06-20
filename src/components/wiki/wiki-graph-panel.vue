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
let graphologyGraph: Graph | null = null;

// ── Filter state ──────────────────────────────────────────────
const hiddenTypes = ref<Set<string>>(new Set());
const searchQuery = ref('');
const legendExpanded = ref(true);
const orphansOnly = ref(false);
const hoveredNode = ref<string | null>(null);

/** Color map for page types. */
const typeColors: Record<string, string> = {
  concept: '#6366f1', // indigo
  author: '#22c55e', // green
  method: '#f97316', // orange
  synthesis: '#a855f7', // purple
  source: '#64748b', // slate
};

/** Legend items for the type colors (with type key for filtering). */
const legendItems = [
  { type: 'concept', label: 'Concepts', color: typeColors.concept! },
  { type: 'author', label: 'Authors', color: typeColors.author! },
  { type: 'method', label: 'Methods', color: typeColors.method! },
  { type: 'synthesis', label: 'Synthesis', color: typeColors.synthesis! },
];

/** Toggle a page type in the hidden set. */
function toggleType(type: string): void {
  const next = new Set(hiddenTypes.value);
  if (next.has(type)) {
    next.delete(type);
  } else {
    next.add(type);
  }
  hiddenTypes.value = next;
  applyFilters();
}

/** Toggle orphans-only filter. */
function toggleOrphans(): void {
  orphansOnly.value = !orphansOnly.value;
  applyFilters();
}

/** Reset all filters to default state. */
function resetFilters(): void {
  searchQuery.value = '';
  hiddenTypes.value = new Set();
  orphansOnly.value = false;
  hoveredNode.value = null;
  applyFilters();
  // Reset zoom and pan to default view.
  if (sigma) {
    const camera = sigma.getCamera();
    camera.animate({ ratio: 1, x: 0.5, y: 0.5 }, { duration: 300 });
  }
}

/** Clear the search query. */
function clearSearch(): void {
  searchQuery.value = '';
  applyFilters();
}

/** Apply type + search + orphan + hover filters to the graphology graph. */
function applyFilters(): void {
  const g = graphologyGraph;
  if (!g) return;
  const q = searchQuery.value.trim().toLowerCase();
  const hov = hoveredNode.value;

  // If hovering, collect connected node set.
  let connectedSet: Set<string> | null = null;
  if (hov && g.hasNode(hov)) {
    connectedSet = new Set([hov]);
    g.forEachNeighbor(hov, (n) => connectedSet!.add(n));
  }

  g.forEachNode((node, attrs) => {
    const pageType = (attrs.pageType as string) ?? '';
    const typeHidden = hiddenTypes.value.has(pageType);
    const label = ((attrs.label as string) ?? '').toLowerCase();
    const labelMatch = q === '' || label.includes(q);

    // Orphans filter: only show nodes with no connections.
    let orphanMatch = true;
    if (orphansOnly.value) {
      const degree = g.degree(node);
      orphanMatch = degree === 0;
    }

    // Hover dimming: dim nodes not connected to hovered node.
    let hoverDim = false;
    if (connectedSet && !connectedSet.has(node)) {
      hoverDim = true;
    }

    const hidden = typeHidden || !labelMatch || !orphanMatch;
    g.setNodeAttribute(node, 'hidden', hidden);
    // Store original color for hover restore.
    const origColor = (attrs.origColor as string) ?? (attrs.color as string);
    g.setNodeAttribute(node, 'origColor', origColor);
    g.setNodeAttribute(node, 'color', hoverDim ? '#e2e8f0' : origColor);
  });

  // Dim edges not connected to hovered node.
  g.forEachEdge((edge, attrs, source, target) => {
    const origColor = (attrs.origColor as string) ?? (attrs.color as string);
    g.setEdgeAttribute(edge, 'origColor', origColor);
    if (connectedSet && (!connectedSet.has(source) || !connectedSet.has(target))) {
      g.setEdgeAttribute(edge, 'color', '#f1f5f9');
    } else {
      g.setEdgeAttribute(edge, 'color', origColor);
    }
  });

  sigma?.refresh();
}

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
      origColor: color,
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
        origColor: '#cbd5e1',
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

  // Store reference for filtering.
  graphologyGraph = g;

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
      arrow: createEdgeArrowProgram({
        lengthToThicknessRatio: 5,
        widenessToThicknessRatio: 4,
      }),
    },
  });

  // Click handler: select the clicked node.
  sigma.on('clickNode', ({ node }) => {
    emit('selectPage', node);
  });

  // Hover handlers: highlight connected nodes + edges.
  sigma.on('enterNode', ({ node }) => {
    hoveredNode.value = node;
    applyFilters();
  });
  sigma.on('leaveNode', () => {
    hoveredNode.value = null;
    applyFilters();
  });

  // Apply any existing filters after render.
  applyFilters();
}

onMounted(loadAndRender);
onUnmounted(() => {
  if (sigma) {
    sigma.kill();
    sigma = null;
  }
  graphologyGraph = null;
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

    <!-- Stats + Search + Legend overlay (top-right) -->
    <div
      v-if="graph && graph.nodes.length > 0"
      class="absolute top-3 right-3 z-10 bg-white/90 backdrop-blur-sm rounded-lg border border-slate-200 shadow-sm p-3 text-xs space-y-2 min-w-[200px]"
    >
      <div class="flex items-center gap-3 font-mono text-slate-600">
        <span>{{ stats.nodes }} nodes</span>
        <span>{{ stats.edges }} edges</span>
        <button
          v-if="stats.orphans > 0"
          type="button"
          class="cursor-pointer transition-all rounded px-1"
          :class="
            orphansOnly
              ? 'text-amber-700 bg-amber-100 ring-1 ring-amber-300 font-semibold'
              : 'text-amber-600 hover:text-amber-700'
          "
          :title="orphansOnly ? 'Show all nodes' : 'Show orphans only'"
          @click="toggleOrphans"
        >
          {{ stats.orphans }} orphans
        </button>
        <button
          type="button"
          class="ml-auto flex items-center cursor-pointer text-slate-400 hover:text-indigo-600 transition-colors"
          title="Reset all filters"
          @click="resetFilters"
        >
          <span class="material-symbols-outlined text-[16px]">restart_alt</span>
        </button>
        <button
          type="button"
          class="flex items-center cursor-pointer text-slate-400 hover:text-slate-600 transition-colors"
          :title="legendExpanded ? 'Collapse' : 'Expand'"
          @click="legendExpanded = !legendExpanded"
        >
          <span class="material-symbols-outlined text-[16px]">{{
            legendExpanded ? 'expand_less' : 'expand_more'
          }}</span>
        </button>
      </div>

      <template v-if="legendExpanded">
        <!-- Search filter -->
        <div class="relative">
          <span
            class="material-symbols-outlined absolute left-2 top-1/2 -translate-y-1/2 text-[14px] text-slate-400 pointer-events-none"
            >search</span
          >
          <input
            v-model="searchQuery"
            type="text"
            placeholder="Filter nodes..."
            class="w-full pl-7 pr-6 py-1 text-xs bg-slate-50 border border-slate-200 rounded-md focus:outline-none focus:ring-2 focus:ring-indigo-400 focus:border-transparent"
            @input="applyFilters"
          />
          <button
            v-if="searchQuery"
            type="button"
            class="absolute right-1.5 top-1/2 -translate-y-1/2 flex items-center justify-center w-4 h-4 text-slate-400 hover:text-slate-600 cursor-pointer"
            title="Clear search"
            @click="clearSearch"
          >
            <span class="material-symbols-outlined text-[14px]">close</span>
          </button>
        </div>

        <!-- Legend (clickable toggles) -->
        <div class="flex items-center gap-2 flex-wrap">
          <button
            v-for="item in legendItems"
            :key="item.type"
            type="button"
            class="flex items-center gap-1.5 cursor-pointer transition-opacity"
            :class="{ 'opacity-40': hiddenTypes.has(item.type) }"
            :title="hiddenTypes.has(item.type) ? `Show ${item.label}` : `Hide ${item.label}`"
            @click="toggleType(item.type)"
          >
            <span
              class="inline-block w-4 h-4 rounded-full transition-all"
              :style="{ backgroundColor: item.color }"
              :class="{
                'ring-2 ring-offset-1 ring-slate-400': !hiddenTypes.has(item.type),
              }"
            ></span>
            <span class="text-slate-600">{{ item.label }}</span>
          </button>
        </div>
      </template>
    </div>

    <!-- Hint (bottom-left) -->
    <div
      v-if="graph && graph.nodes.length > 0"
      class="absolute bottom-3 left-3 z-10 text-[10px] text-slate-400 bg-white/80 px-2 py-1 rounded"
    >
      Click a node to open its page. Hover to highlight connections. Drag to pan. Scroll to zoom.
    </div>
  </div>
</template>
