<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useTrendsQueueStore } from '../stores/trends-queue';
import GoogleTrendsPanel from '../components/google-trends-panel.vue';
import KeywordNetworkGraph from '../components/keyword-network-graph.vue';
import KeywordControls from '../components/keyword-controls.vue';
import KeywordDetailPanel from '../components/keyword-detail-panel.vue';
import ClusterThemesPanel from '../components/cluster-themes-panel.vue';
import ArticleDetailSlideOver from '../components/article-detail-slide-over.vue';
import { useKeywordNetwork } from '../composables/use-keyword-network';
import { useNetworkView } from '../composables/use-network-view';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { useThemesPanel } from '../composables/use-cluster-themes';
import { useArticleDetailOverlay } from '../composables/use-article-detail-overlay';
import { buildBiblioArticleQuery } from '@/utils/biblio-links';
import '../styles/biblio-chrome.css';
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

/* ── Cluster thematic analysis ─────────────────────────────────────────
 * Shared panel wiring: the canonical LLM gate, the per-cluster session
 * cache, and the open/reanalyze/copy actions (see use-cluster-themes.ts). */
const {
  llmReady,
  themesPanelOpen,
  themesClusterIndex,
  themesEntry,
  analyzeLoading,
  onAnalyzeThemes,
  onReanalyzeThemes,
  onCopyThemes,
} = useThemesPanel({
  networkType: 'co_occurrence',
  recalculateTrigger,
  graph,
  selectedClusters,
});

/* ── Article detail slide-over ───────────────────────────────────────────
 * Shared component + shared overlay guards: `article:` links open the full
 * article detail without leaving the view; closing returns to the exact
 * network state (graph, cluster selection, cached analysis). */
const {
  articleDetailRef,
  showArticleDetail,
  isArticleDetailFullScreen,
  onArticleDetailOpened,
  onArticleDetailClosed,
  onArticleDetailToggleFullScreen,
} = useArticleDetailOverlay();

/* Protocol registry injected into the panel: the keyword network has no
 * author entities, so the registry carries `article` only (the backend prompt
 * teaches the same restricted protocol set via `link_protocols_for`); article
 * links open the full in-view detail slide-over (no route change, so the
 * network state - graph, cluster selection, cached analysis - is intact on
 * close). */
const themeLinkHandlers: Record<string, (id: string) => void> = {
  article: (id: string) => {
    void articleDetailRef.value?.open(id);
  },
};

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
 * Deep-link to article list filtered by selected keyword. Source-aware:
 * only `tags`/`labels`-sourced nodes can be matched by existing filters.
 * Routes through `buildBiblioArticleQuery` which enforces `status: 'included'`.
 */
function viewKeywordArticles(): void {
  const keyword = selectedKeyword.value;
  if (!keyword) return;
  /* Defensive: detail panel gates button to these sources, but guard here
   * too so future caller cannot route deferred source through wrong filter. */
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
      { year: number; count: number }[] | undefined;
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
  <div class="network-view-layout">
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
          :llm-ready="llmReady"
          :analysis-loading="analyzeLoading"
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
          @analyze-themes="onAnalyzeThemes"
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

    <!-- Central Content Area (hidden while the article detail overlay is fullscreen) -->
    <div
      v-show="!(showArticleDetail && isArticleDetailFullScreen)"
      class="flex-1 flex flex-col min-h-0 relative"
    >
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

    <!-- Keyword detail panel (hidden while the article detail overlay is open) -->
    <Transition name="detail-slide">
      <KeywordDetailPanel
        v-if="selectedKeyword && !showArticleDetail"
        :keyword="selectedKeyword"
        :graph="graph"
        class="w-72 shrink-0"
        @close="onNodeClick(null)"
        @navigate="onNavigateToKeyword"
        @view-articles="viewKeywordArticles"
      />
    </Transition>

    <!-- Cluster thematic analysis panel -->
    <ClusterThemesPanel
      :visible="themesPanelOpen"
      :title="`Cluster ${(themesClusterIndex ?? 0) + 1} - Thematic Analysis`"
      :markdown="themesEntry.markdown"
      :loading="themesEntry.loading"
      :error="themesEntry.error"
      :link-handlers="themeLinkHandlers"
      @close="themesPanelOpen = false"
      @reanalyze="onReanalyzeThemes"
      @copy="onCopyThemes"
    />

    <!-- Full article detail panel (opened via the themes panel article links). -->
    <ArticleDetailSlideOver
      ref="articleDetailRef"
      :full-screen="isArticleDetailFullScreen"
      @opened="onArticleDetailOpened"
      @closed="onArticleDetailClosed"
      @toggle-full-screen="onArticleDetailToggleFullScreen"
    />
  </div>
</template>
