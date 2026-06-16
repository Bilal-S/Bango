<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import Graph from 'graphology';
import CocitationNetworkGraph from '../components/cocitation-network-graph.vue';
import CocitationControls from '../components/cocitation-controls.vue';
import CocitationDetailPanel from '../components/cocitation-detail-panel.vue';
import CocitationHeatmap from '../components/cocitation-heatmap.vue';
import { useCocitationNetwork } from '../composables/use-cocitation-network';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { exportNetworkPng, exportNetworkGexf } from '../utils/network-export';
import type { NetworkExportFormat } from '../utils/network-export';
import type { CocitationNode, CocitationEdge } from '../types/biblio-cocitation';

const {
  graph,
  loading,
  error,
  meta,
  nodeCount,
  edgeCount,
  fetchNetwork,
  getNode,
  getCoCitedPapers,
} = useCocitationNetwork();

const { isLayouting, applyLayout } = useNetworkLayout();
const { applyKeywordGraphFilters } = useSigmaRenderer();

const graphRef = ref<InstanceType<typeof CocitationNetworkGraph> | null>(null);

function locateNode(nodeId: string) {
  graphRef.value?.locateNode(nodeId);
}
function resetZoom() {
  graphRef.value?.resetZoom();
}
function refresh() {
  graphRef.value?.refresh();
}

// Control state
const scope = ref<'included' | 'all'>('included');
const normalization = ref<'raw' | 'cosine' | 'jaccard' | 'pearson'>('cosine');
const minCitationCount = ref(2);
const minCoCitation = ref(2);
const selectedPaper = ref<CocitationNode | null>(null);
const focusedNodeId = ref<string | null>(null);
const visibleNodeCount = ref(0);
const colorMode = ref<'cluster' | 'temporal'>('cluster');
const layoutMode = ref<'fixed' | 'dynamic'>('fixed');
const selectedClusters = ref<number[]>([]);
const sidebarCollapsed = ref(false);
const showHeatmap = ref(false);
const recalculateTrigger = ref(0);

/** Derive cluster count from graph node attributes (visible nodes only). */
const clusterCount = computed(() => {
  void recalculateTrigger.value;
  void visibleNodeCount.value;
  if (!graph.value) return 0;
  const clusters = new Set<number>();
  graph.value.forEachNode((node) => {
    const isHidden = graph.value!.getNodeAttribute(node, 'hidden') as boolean | null;
    if (isHidden !== true) {
      const c = graph.value!.getNodeAttribute(node, 'cluster') as number | null;
      if (c !== null && c !== undefined) clusters.add(c);
    }
  });
  return clusters.size;
});

/** Year range from graph for temporal color gradient. */
const yearRange = computed(() => {
  if (!graph.value) return { min: 2000, max: 2024 };
  let min = Infinity;
  let max = -Infinity;
  graph.value.forEachNode((node) => {
    const yr = graph.value!.getNodeAttribute(node, 'year') as number | null;
    if (yr !== null && yr !== undefined) {
      if (yr < min) min = yr;
      if (yr > max) max = yr;
    }
  });
  if (min === Infinity || max === -Infinity) return { min: 2000, max: 2024 };
  if (min === max) return { min: min - 1, max: min + 1 };
  return { min: Math.floor(min), max: Math.ceil(max) };
});

const stats = computed(() => ({
  totalNodes: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleNodes: visibleNodeCount.value || nodeCount.value,
  clusterCount: clusterCount.value,
}));

/** Paper search entries for autocomplete search.
 * Each entry contains:
 * - `label`: the author (year) string used to locate the node
 * - `display`: truncated title + label for the dropdown display
 * - `searchText`: lowercase concatenation of all searchable fields
 */
const paperLabels = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id: string) => {
    const attrs = graph.value!.getNodeAttributes(id);
    const label = (attrs.label as string) ?? id;
    const title = (attrs.title as string) ?? '';
    const authors = (attrs.authors as string) ?? '';
    const doi = (attrs.doi as string) ?? '';
    const shortTitle = title.length > 15 ? title.slice(0, 15) + '…' : title;
    return {
      label,
      display: shortTitle ? `${shortTitle}:${label}` : label,
      searchText: [label, title, authors, doi].join(' ').toLowerCase(),
    };
  });
});

/** Co-cited partners for detail panel. */
const coCitedPapers = computed(() => {
  if (!selectedPaper.value) return [];
  return getCoCitedPapers(selectedPaper.value.id);
});

/** Raw node/edge arrays for the heatmap (derived from graph). */
const heatmapNodes = computed<CocitationNode[]>(() => {
  if (!graph.value) return [];
  const result: CocitationNode[] = [];
  graph.value.forEachNode((id) => {
    const attrs = graph.value!.getNodeAttributes(id);
    result.push({
      id,
      label: attrs.label,
      title: attrs.title,
      authors: attrs.authors,
      year: attrs.year,
      journal: attrs.journal,
      doi: attrs.doi,
      citationCount: attrs.citationCount,
      coCitationCount: attrs.coCitationCount,
      matchedArticleId: attrs.matchedArticleId,
      abstract: attrs.abstract,
      referenceType: attrs.referenceType,
    });
  });
  return result;
});

const heatmapEdges = computed<CocitationEdge[]>(() => {
  if (!graph.value) return [];
  const result: CocitationEdge[] = [];
  graph.value.forEachEdge((edgeKey) => {
    const attrs = graph.value!.getEdgeAttributes(edgeKey);
    const [source, target] = graph.value!.extremities(edgeKey);
    result.push({
      source,
      target,
      weight: attrs.weight,
      rawWeight: attrs.rawWeight,
      cosineWeight: attrs.cosineWeight,
      jaccardWeight: attrs.jaccardWeight,
      pearsonWeight: attrs.pearsonWeight,
    });
  });
  return result;
});

onMounted(async () => {
  await doFetch();
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    recalculateTrigger.value++;
  }
});

async function doFetch() {
  await fetchNetwork({
    scope: scope.value,
    normalization: normalization.value,
    minCitationCount: minCitationCount.value,
    minCoCitation: minCoCitation.value,
  });
}

function onNodeClick(nodeId: string | null) {
  focusedNodeId.value = nodeId;
  if (!nodeId) {
    selectedPaper.value = null;
    return;
  }
  selectedPaper.value = getNode(nodeId);
}

function onNavigateToPaper(nodeId: string) {
  onNodeClick(nodeId);
  locateNode(nodeId);
}

async function onParamsChange() {
  selectedPaper.value = null;
  focusedNodeId.value = null;
  selectedClusters.value = [];
  await doFetch();
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    recalculateTrigger.value++;
  }
}

async function onLayoutModeChange(mode: 'fixed' | 'dynamic') {
  layoutMode.value = mode;
  if (graph.value) {
    await onRecalculate();
  }
}

function onFilterChange(filters: { search: string }) {
  if (!graph.value) return;
  const result = applyKeywordGraphFilters(graph.value, {
    minOccurrences: 0,
    minCooccurrence: 0,
    search: filters.search,
  });
  visibleNodeCount.value = result.visibleNodes;
}

function onSelectCluster(clusterId: number) {
  const idx = selectedClusters.value.indexOf(clusterId);
  if (idx >= 0) {
    selectedClusters.value.splice(idx, 1);
  } else {
    selectedClusters.value.push(clusterId);
  }
}

function onClearClusters() {
  selectedClusters.value = [];
}

async function onExportImage(format: NetworkExportFormat) {
  try {
    if (format === 'png') {
      if (!graphRef.value?.renderer) return;
      await exportNetworkPng(graphRef.value.renderer, 'cocitation-network.png');
    } else if (format === 'gexf') {
      if (!graph.value) return;
      await exportNetworkGexf(graph.value, 'cocitation-network.gexf');
    }
  } catch (err) {
    console.error('[export] Co-citation network export failed:', err);
  }
}

function onLocatePaper(label: string) {
  if (!graph.value) return;
  const nodeId = graph.value.findNode(
    (node) => (graph.value!.getNodeAttribute(node, 'label') as string) === label
  );
  if (nodeId) {
    onNodeClick(nodeId);
    locateNode(nodeId);
  }
}

async function onResetAnalysis() {
  colorMode.value = 'cluster';
  layoutMode.value = 'fixed';
  scope.value = 'included';
  normalization.value = 'cosine';
  minCitationCount.value = 2;
  minCoCitation.value = 2;
  selectedClusters.value = [];
  selectedPaper.value = null;
  focusedNodeId.value = null;
  await onParamsChange();
}

async function onRecalculate() {
  if (!graph.value) return;

  isLayouting.value = true;
  try {
    const sub = new Graph({ type: 'undirected', multi: false });

    graph.value.forEachNode((node, attrs) => {
      if (attrs.hidden !== true) {
        sub.addNode(node, { ...attrs });
      }
    });

    graph.value.forEachEdge((edge, attrs, source, target) => {
      if (attrs.hidden !== true && sub.hasNode(source) && sub.hasNode(target)) {
        sub.addUndirectedEdgeWithKey(edge, source, target, { ...attrs });
      }
    });

    if (sub.order === 0) return;

    await applyLayout(sub, 150, layoutMode.value);

    sub.forEachNode((node) => {
      const newAttrs = sub.getNodeAttributes(node);
      graph.value!.setNodeAttribute(node, 'x', newAttrs.x);
      graph.value!.setNodeAttribute(node, 'y', newAttrs.y);
      graph.value!.setNodeAttribute(node, 'cluster', newAttrs.cluster);
    });

    resetZoom();
    refresh();
    recalculateTrigger.value++;
  } finally {
    isLayouting.value = false;
  }
}
</script>

<template>
  <div class="cocitation-layout">
    <!-- Sidebar Wrapper -->
    <div
      class="sidebar-wrapper relative transition-all duration-300 shrink-0"
      :class="sidebarCollapsed ? 'w-0' : 'w-72'"
    >
      <aside
        class="sidebar-panel h-full p-4 overflow-y-auto border-r border-slate-100 bg-slate-50/30 transition-all duration-300 flex flex-col"
        :class="sidebarCollapsed ? 'w-0 p-0 overflow-hidden opacity-0' : 'w-full opacity-100'"
      >
        <CocitationControls
          class="my-auto"
          :total-nodes="stats.totalNodes"
          :total-edges="stats.totalEdges"
          :visible-nodes="stats.visibleNodes"
          :cluster-count="stats.clusterCount"
          :scope="scope"
          :normalization="normalization"
          :min-citation-count="minCitationCount"
          :min-co-citation="minCoCitation"
          :color-mode="colorMode"
          :layout-mode="layoutMode"
          :paper-labels="paperLabels"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :selected-clusters="selectedClusters"
          @scope-change="
            scope = $event;
            onParamsChange();
          "
          @normalization-change="
            normalization = $event as 'raw' | 'cosine' | 'jaccard' | 'pearson';
            onParamsChange();
          "
          @min-citation-change="
            minCitationCount = $event;
            onParamsChange();
          "
          @min-co-citation-change="
            minCoCitation = $event;
            onParamsChange();
          "
          @color-mode-change="colorMode = $event"
          @layout-mode-change="onLayoutModeChange"
          @locate-paper="onLocatePaper"
          @filter-change="onFilterChange"
          @select-cluster="onSelectCluster($event)"
          @clear-clusters="onClearClusters"
          @export-image="onExportImage"
          @recalculate="onRecalculate"
          @reset-analysis="onResetAnalysis"
        />
      </aside>

      <!-- Drawer handle -->
      <button
        class="drawer-handle"
        :title="sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'"
        :style="{ left: sidebarCollapsed ? '0px' : 'calc(100% - 16px)' }"
        @click="sidebarCollapsed = !sidebarCollapsed"
      >
        <span class="drawer-handle-grip"></span>
      </button>
    </div>

    <!-- Central Content Area -->
    <div class="flex-1 flex flex-col min-h-0 relative">
      <!-- Graph canvas -->
      <main class="flex-1 relative min-h-0">
        <CocitationNetworkGraph
          ref="graphRef"
          :graph="graph"
          :loading="loading"
          :is-layouting="isLayouting"
          :error="error"
          :focused-node-id="focusedNodeId"
          :selected-clusters="selectedClusters"
          :color-mode="colorMode"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :recalculate-trigger="recalculateTrigger"
          @node-click="onNodeClick"
          @retry="doFetch"
        />

        <!-- Heatmap toggle button -->
        <button
          v-if="graph && nodeCount > 0"
          class="absolute top-3 right-3 z-10 flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-white border border-slate-200 rounded-lg shadow-sm hover:bg-slate-50 transition-colors"
          @click="showHeatmap = !showHeatmap"
        >
          <span class="material-symbols-outlined text-sm">{{
            showHeatmap ? 'grid_off' : 'grid_on'
          }}</span>
          {{ showHeatmap ? 'Hide' : 'Heatmap' }}
        </button>

        <!-- Diagnostic empty-state -->
        <div
          v-if="!loading && !error && meta && meta.edgeCount === 0 && meta.referencePaperCount > 0"
          class="absolute bottom-4 left-1/2 -translate-x-1/2 z-30 max-w-md bg-amber-50 border border-amber-200 rounded-xl shadow-sm p-4 flex gap-3"
        >
          <span class="material-symbols-outlined text-amber-500 text-xl shrink-0">info</span>
          <div class="text-xs text-amber-800">
            <p class="font-semibold mb-0.5">No co-citation pairs found</p>
            <p class="leading-relaxed">
              {{ meta.referencePaperCount }} reference papers were imported, but none are cited
              together by at least {{ minCoCitation }} of your
              {{ meta.inScopeArticleCount }} in-scope articles.
            </p>
            <p class="mt-1.5 leading-relaxed text-amber-700">
              Try lowering the
              <span class="font-medium">minimum co-citation count</span> or the
              <span class="font-medium">minimum citation count</span> threshold.
            </p>
          </div>
        </div>
      </main>

      <!-- Heatmap drawer (collapsible bottom panel) -->
      <Transition name="heatmap-slide">
        <div
          v-if="showHeatmap && graph && nodeCount > 0"
          class="heatmap-drawer border-t border-slate-200 bg-white px-4 py-3 max-h-[360px] overflow-y-auto"
        >
          <CocitationHeatmap
            :nodes="heatmapNodes"
            :edges="heatmapEdges"
            @toggle="showHeatmap = false"
          />
        </div>
      </Transition>
    </div>

    <!-- Detail panel -->
    <Transition name="detail-slide">
      <CocitationDetailPanel
        v-if="selectedPaper"
        :paper="selectedPaper"
        :co-cited-papers="coCitedPapers"
        class="w-72 shrink-0"
        @close="onNodeClick(null)"
        @navigate-paper="onNavigateToPaper"
      />
    </Transition>
  </div>
</template>

<style scoped>
.cocitation-layout {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}

.sidebar-panel {
  z-index: 20;
}

/* Drawer handle */
.drawer-handle {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
  z-index: 30;
  width: 14px;
  height: 72px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-surface-container-low);
  border: 1px solid var(--color-outline-variant);
  border-left: none;
  border-radius: 0 8px 8px 0;
  box-shadow: 2px 0 4px rgba(0, 0, 0, 0.06);
  cursor: pointer;
  transition:
    left 0.3s,
    background-color 0.15s,
    border-color 0.15s,
    width 0.15s;
}

.drawer-handle:hover {
  background: var(--color-surface-container);
  border-color: var(--color-primary);
  width: 16px;
}

.drawer-handle-grip {
  display: flex;
  flex-direction: column;
  gap: 3px;
  align-items: center;
}

.drawer-handle-grip::before,
.drawer-handle-grip::after,
.drawer-handle-grip {
  content: '';
  display: block;
  width: 4px;
  height: 2px;
  border-radius: 1px;
  background: #94a3b8;
  transition: background-color 0.15s;
}

.drawer-handle:hover .drawer-handle-grip::before,
.drawer-handle:hover .drawer-handle-grip::after,
.drawer-handle:hover .drawer-handle-grip {
  background: var(--color-primary);
}

/* Heatmap slide transition */
.heatmap-slide-enter-active,
.heatmap-slide-leave-active {
  transition:
    max-height 0.25s ease,
    opacity 0.25s ease;
}

.heatmap-slide-enter-from,
.heatmap-slide-leave-to {
  max-height: 0;
  opacity: 0;
  overflow: hidden;
}

/* Detail panel slide transition */
.detail-slide-enter-active,
.detail-slide-leave-active {
  transition:
    transform 0.25s ease,
    opacity 0.25s ease;
}

.detail-slide-enter-from,
.detail-slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}
</style>
