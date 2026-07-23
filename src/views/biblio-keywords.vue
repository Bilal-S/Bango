<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useTrendsQueueStore } from '../stores/trends-queue';
import GoogleTrendsPanel from '../components/google-trends-panel.vue';
import KeywordNetworkGraph from '../components/keyword-network-graph.vue';
import KeywordControls from '../components/keyword-controls.vue';
import KeywordDetailPanel from '../components/keyword-detail-panel.vue';
import { useKeywordNetwork } from '../composables/use-keyword-network';
import { useNetworkView } from '../composables/use-network-view';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { buildBiblioArticleQuery } from '@/utils/biblio-links';
import type { NetworkExportFormat } from '../utils/network-export';
import type { KeywordNode } from '../types/biblio-keyword';

const router = useRouter();

const {
  graph,
  loading,
  error,
  nodeCount,
  edgeCount,
  clusterCount: networkClusterCount,
  isLayouting,
  sources,
  minOccurrences,
  minCooccurrence,
  fetchNetwork,
} = useKeywordNetwork();

const {
  graphRef,
  focusedNodeId,
  visibleNodeCount,
  visibleEdgeCount,
  colorMode,
  layoutMode,
  selectedClusters,
  sidebarCollapsed,
  recalculateTrigger,
  clusterCount,
  yearRange,
  focusNode,
  locateByLabel,
  onSelectCluster,
  onClearClusters,
  onLayoutModeChange,
  onExportImage: exportImage,
  onRecalculate,
  resetViewState,
} = useNetworkView({
  graph,
  exportPrefix: 'keyword-network',
  yearAttribute: 'avgYear',
  defaultYearRange: { min: 2000, max: 2026 },
  recalculateIterations: 150,
});

const { applyKeywordGraphFilters } = useSigmaRenderer();

const selectedKeyword = ref<KeywordNode | null>(null);

/** Year range from graph nodes' avgYear for temporal color gradient */
// yearRange from composable is used directly (configured for avgYear).

const stats = computed(() => ({
  totalNodes: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleNodes: visibleNodeCount.value || nodeCount.value,
  visibleEdges: visibleEdgeCount.value || edgeCount.value,
  clusterCount: networkClusterCount.value || clusterCount.value,
}));

/** Keyword labels for autocomplete search */
const keywordLabels = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return attrs.label ?? id;
  });
});

onMounted(async () => {
  await fetchNetwork(layoutMode.value);
  if (graph.value) {
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
});

function onNodeClick(nodeId: string | null) {
  focusNode(nodeId);
  if (!nodeId) {
    selectedKeyword.value = null;
    return;
  }
  if (graph.value && graph.value.hasNode(nodeId)) {
    const attrs = graph.value.getNodeAttributes(nodeId);
    selectedKeyword.value = {
      id: nodeId,
      label: attrs.label ?? nodeId,
      weight: attrs.weight ?? 0,
      source: attrs.source ?? '',
      avgYear: attrs.avgYear ?? null,
      rawTerms: attrs.rawTerms ?? [],
      cluster: attrs.cluster ?? null,
      yearCounts: attrs.yearCounts ?? [],
    };
  }
}

function onNavigateToKeyword(nodeId: string) {
  onNodeClick(nodeId);
  graphRef.value?.locateNode(nodeId);
}

/**
 * Deep-link to the article list filtered by the selected keyword (Gap 1a).
 *
 * Source-aware routing: the keyword network draws nodes from multiple
 * sources (`metadata | ai_extracted | user_added | tags | labels`). Only
 * `tags`/`labels`-sourced nodes can be matched by the existing
 * `ArticleQuery.tags` / `ArticleQuery.labels` filters (the node label is the
 * tag/label name). The detail panel gates the "View articles" button to
 * those sources via `canViewArticles`, so this handler only fires for them.
 * `metadata`/`ai_extracted`/`user_added`-sourced nodes are deferred to
 * Gap 1b (backend `ArticleQuery.keywords` + `json_each()`).
 *
 * Routes through `buildBiblioArticleQuery`, which enforces
 * `status: 'included'` (decision D1) in one place — the keyword network is
 * scoped to included articles.
 */
function viewKeywordArticles(): void {
  const keyword = selectedKeyword.value;
  if (!keyword) return;
  // Defensive: the detail panel gates the button to these sources, but guard
  // here too so a future caller cannot route a deferred source through the
  // wrong filter.
  if (keyword.source !== 'tags' && keyword.source !== 'labels') return;
  const filter =
    keyword.source === 'tags' ? { tags: [keyword.label] } : { labels: [keyword.label] };
  void router.push(buildBiblioArticleQuery('keywords', filter));
}

async function onParamsChange(params: {
  sources: string[];
  minOccurrences: number;
  minCooccurrence: number;
}) {
  sources.value = params.sources;
  minOccurrences.value = params.minOccurrences;
  minCooccurrence.value = params.minCooccurrence;

  selectedKeyword.value = null;
  focusNode(null);
  selectedClusters.value = [];

  await fetchNetwork(layoutMode.value);
  if (graph.value) {
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
}

function onFilterChange(filters: {
  minOccurrences: number;
  minCooccurrence: number;
  search: string;
}) {
  if (!graph.value) return;
  const result = applyKeywordGraphFilters(graph.value, filters);
  visibleNodeCount.value = result.visibleNodes;
  visibleEdgeCount.value = result.visibleEdges;
}

async function onExportImage(format: NetworkExportFormat) {
  const renderer = (graphRef.value as { renderer?: unknown } | null)?.renderer;
  await exportImage(format, (renderer as Parameters<typeof exportImage>[1]) ?? null);
}

async function onResetAnalysis() {
  resetViewState();
  selectedKeyword.value = null;

  // Set defaults and re-fetch
  sources.value = ['metadata', 'ai_extracted', 'tags', 'labels', 'user_added'];
  minOccurrences.value = 2;
  minCooccurrence.value = 2;

  await fetchNetwork(layoutMode.value);
  if (graph.value) {
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
}

const trendsQueue = useTrendsQueueStore();

const datasetYearsStats = computed(() => {
  if (!graph.value) return null;
  const yearCountsMap = new Map<number, number>();

  graph.value.forEachNode((node) => {
    const yc = graph.value!.getNodeAttribute(node, 'yearCounts') as
      | { year: number; count: number }[]
      | undefined;
    if (yc) {
      for (const item of yc) {
        yearCountsMap.set(item.year, (yearCountsMap.get(item.year) || 0) + item.count);
      }
    }
  });

  if (yearCountsMap.size === 0) return null;

  let minYear = Infinity;
  let maxYear = -Infinity;
  let mostActiveYear = 2002;
  let maxCount = -1;

  for (const [year, count] of yearCountsMap.entries()) {
    if (year < minYear) minYear = year;
    if (year > maxYear) maxYear = year;
    if (count > maxCount) {
      maxCount = count;
      mostActiveYear = year;
    }
  }

  return { minYear, maxYear, mostActiveYear };
});

watch(
  datasetYearsStats,
  (newStats) => {
    if (newStats) {
      trendsQueue.setResearchRange(newStats.minYear, newStats.maxYear, newStats.mostActiveYear);
    }
  },
  { immediate: true }
);
</script>

<template>
  <div class="keyword-layout">
    <!-- Sidebar Wrapper -->
    <div
      class="sidebar-wrapper relative transition-all duration-300 shrink-0"
      :class="sidebarCollapsed ? 'w-0' : 'w-64'"
    >
      <!-- Controls sidebar -->
      <aside
        class="sidebar-panel h-full p-4 overflow-y-auto border-r border-slate-100 bg-slate-50/30 transition-all duration-300 flex flex-col"
        :class="sidebarCollapsed ? 'w-0 p-0 overflow-hidden opacity-0' : 'w-full opacity-100'"
      >
        <KeywordControls
          class="my-auto"
          :total-nodes="stats.totalNodes"
          :total-edges="stats.totalEdges"
          :visible-nodes="stats.visibleNodes"
          :visible-edges="stats.visibleEdges"
          :cluster-count="stats.clusterCount"
          :keyword-labels="keywordLabels"
          :color-mode="colorMode"
          :layout-mode="layoutMode"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :selected-clusters="selectedClusters"
          :sources="sources"
          :min-occurrences="minOccurrences"
          :min-cooccurrence="minCooccurrence"
          @filter-change="onFilterChange"
          @params-change="onParamsChange"
          @layout-mode-change="onLayoutModeChange"
          @locate-keyword="(label: string) => locateByLabel(label)"
          @export-image="onExportImage"
          @color-mode-change="colorMode = $event"
          @select-cluster="onSelectCluster($event)"
          @clear-clusters="onClearClusters"
          @fit-screen="() => graphRef?.resetZoom()"
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
        <KeywordNetworkGraph
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
          @retry="fetchNetwork"
        />
      </main>

      <!-- Google Trends bottom drawer panel -->
      <GoogleTrendsPanel />
    </div>

    <!-- Keyword detail panel -->
    <Transition name="detail-slide">
      <KeywordDetailPanel
        v-if="selectedKeyword"
        :keyword="selectedKeyword"
        :graph="graph"
        class="w-72 shrink-0"
        @close="onNodeClick(null)"
        @navigate="onNavigateToKeyword"
        @view-articles="viewKeywordArticles"
      />
    </Transition>
  </div>
</template>

<style scoped>
.keyword-layout {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}

.sidebar-panel {
  z-index: 20;
}

/* Drawer handle - small pill tab positioned at sidebar edge */
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

/* Grip dots inside the handle */
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
