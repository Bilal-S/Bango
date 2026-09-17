<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import NetworkGraph from '../components/network-graph.vue';
import NetworkControls from '../components/network-controls.vue';
import AuthorDetailPanel from '../components/author-detail-panel.vue';
import ClusterThemesPanel from '../components/cluster-themes-panel.vue';
import ArticleDetailSlideOver from '../components/article-detail-slide-over.vue';
import { useCoAuthorNetwork } from '../composables/use-coauthor-network';
import { useNetworkView } from '../composables/use-network-view';
import { useNetworkLayout } from '../composables/use-network-layout';
import { useSigmaRenderer } from '../composables/use-sigma-renderer';
import { useThemesPanel } from '../composables/use-cluster-themes';
import { useArticleDetailOverlay } from '../composables/use-article-detail-overlay';
import { buildBiblioArticleQuery } from '@/utils/biblio-links';
import '../styles/biblio-chrome.css';
import type { NetworkExportFormat } from '../utils/network-export';
import type { CoAuthorNode } from '../types/biblio-network';

const router = useRouter();

const { graph, loading, error, nodeCount, edgeCount, countingMode, fetchNetwork, setCountingMode } =
  useCoAuthorNetwork();

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
  isLayouting,
  applyLayout,
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
  exportPrefix: 'coauthor-network',
  yearAttribute: 'avgYear',
});

const { runForceAtlas2Async } = useNetworkLayout();
const { applyGraphFilters } = useSigmaRenderer();

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
  networkType: 'co_authorship',
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

/* Protocol registry injected into the panel: author -> focus + locate,
 * article -> full in-view article detail slide-over (no route change, so the
 * network state - graph, cluster selection, cached analysis - is intact on
 * close). */
const themeLinkHandlers: Record<string, (id: string) => void> = {
  author: (id: string) => onNavigateToAuthor(id),
  article: (id: string) => {
    void articleDetailRef.value?.open(id);
  },
};

const selectedAuthor = ref<CoAuthorNode | null>(null);

const stats = computed(() => ({
  totalAuthors: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleAuthors: visibleNodeCount.value || nodeCount.value,
  visibleEdges: visibleEdgeCount.value || edgeCount.value,
  clusterCount: clusterCount.value,
}));

const authorNames = computed(() => {
  if (!graph.value) return [];
  return graph.value.nodes().map((id: string) => {
    const attrs = graph.value!.getNodeAttributes(id);
    return attrs.label ?? id;
  });
});

const authorWeights = computed(() => {
  const map = new Map<string, number>();
  if (!graph.value) return map;
  for (const id of graph.value.nodes()) {
    const attrs = graph.value.getNodeAttributes(id);
    map.set(attrs.label ?? id, attrs.weight ?? 0);
  }
  return map;
});

onMounted(async () => {
  await fetchNetwork();
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    visibleEdgeCount.value = edgeCount.value;
    recalculateTrigger.value++;
  }
});

function onNodeClick(nodeId: string | null) {
  focusNode(nodeId);
  if (!nodeId || !graph.value) {
    selectedAuthor.value = null;
    return;
  }
  const attrs = graph.value.getNodeAttributes(nodeId);
  selectedAuthor.value = {
    id: nodeId,
    label: attrs.label ?? nodeId,
    weight: attrs.weight ?? 0,
    totalCitations: attrs.totalCitations ?? 0,
    avgYear: attrs.avgYear ?? null,
    estimatedHIndex: attrs.estimatedHIndex ?? null,
    cluster: attrs.cluster ?? null,
  };
}

function onNavigateToAuthor(nodeId: string) {
  onNodeClick(nodeId);
  graphRef.value?.locateNode(nodeId);
}

function onFilterChange(filters: {
  minPapers: number;
  minLinkStrength: number;
  maxAuthors: number;
  search: string;
}) {
  if (!graph.value) return;
  const result = applyGraphFilters(graph.value, filters);
  visibleNodeCount.value = result.visibleNodes;
  visibleEdgeCount.value = result.visibleEdges;
}

async function onExportImage(format: NetworkExportFormat) {
  const renderer = (graphRef.value as { renderer?: unknown } | null)?.renderer;
  await exportImage(format, (renderer as Parameters<typeof exportImage>[1]) ?? null);
}

async function onCountingModeChange(mode: 'full' | 'fractional') {
  const changed = setCountingMode(mode);
  if (changed && graph.value) {
    await runForceAtlas2Async(graph.value, 30, layoutMode.value);
  }
}

async function onResetAnalysis() {
  resetViewState();
  selectedAuthor.value = null;
  const changed = setCountingMode('full');
  if (changed && graph.value) {
    await runForceAtlas2Async(graph.value, 30, layoutMode.value);
  }
  await onRecalculate();
}

/**
 * Deep-link to the article list filtered by the selected author. The
 * co-authorship graph is scoped to included articles, so the filter-based
 * deep-link routes through `buildBiblioArticleQuery`, which enforces
 * `status: 'included'` (decision D1) in one place.
 */
function viewAuthorArticles(): void {
  if (!selectedAuthor.value) return;
  void router.push(buildBiblioArticleQuery('coauthors', { author: selectedAuthor.value.label }));
}
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
        <NetworkControls
          class="my-auto"
          :total-nodes="stats.totalAuthors"
          :total-edges="stats.totalEdges"
          :visible-nodes="stats.visibleAuthors"
          :visible-edges="stats.visibleEdges"
          :cluster-count="stats.clusterCount"
          :author-names="authorNames"
          :author-weights="authorWeights"
          :counting-mode="countingMode"
          :color-mode="colorMode"
          :layout-mode="layoutMode"
          :min-year="yearRange.min"
          :max-year="yearRange.max"
          :selected-clusters="selectedClusters"
          :llm-ready="llmReady"
          :analysis-loading="analyzeLoading"
          @filter-change="onFilterChange"
          @locate-author="(name: string) => locateByLabel(name)"
          @export-image="onExportImage"
          @counting-mode-change="onCountingModeChange"
          @color-mode-change="colorMode = $event"
          @layout-mode-change="onLayoutModeChange"
          @select-cluster="onSelectCluster($event)"
          @clear-clusters="onClearClusters"
          @analyze-themes="onAnalyzeThemes"
          @recalculate="onRecalculate"
          @reset-analysis="onResetAnalysis"
        />
      </aside>

      <!-- Drawer handle (inside wrapper to be relative to it, but outside aside to avoid overflow clipping) -->
      <button
        class="drawer-handle"
        :title="sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'"
        :style="{ left: sidebarCollapsed ? '0px' : 'calc(100% - 16px)' }"
        @click="sidebarCollapsed = !sidebarCollapsed"
      >
        <span class="drawer-handle-grip"></span>
      </button>
    </div>

    <!-- Graph canvas (hidden while the article detail overlay is fullscreen) -->
    <main v-show="!(showArticleDetail && isArticleDetailFullScreen)" class="flex-1 relative">
      <NetworkGraph
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

    <!-- Detail panel (hidden while the article detail overlay is open) -->
    <AuthorDetailPanel
      v-show="!showArticleDetail"
      :author="selectedAuthor"
      :graph="graph"
      @close="onNodeClick(null)"
      @navigate="onNavigateToAuthor"
      @view-articles="viewAuthorArticles"
    />

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
