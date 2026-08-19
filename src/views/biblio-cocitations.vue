<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import CocitationNetworkGraph from '../components/cocitation-network-graph.vue';
import CocitationControls from '../components/cocitation-controls.vue';
import CocitationDetailPanel from '../components/cocitation-detail-panel.vue';
import CocitationHeatmap from '../components/cocitation-heatmap.vue';
import ArticleDetailPanel from '../components/article-detail-panel.vue';
import { useCocitationNetwork } from '../composables/use-cocitation-network';
import { useNetworkView } from '../composables/use-network-view';
import { applyCocitationGraphFilters, applyRejectedMatchesFilter } from '../utils/graph-filters';
import { useArticleSearch } from '../composables/use-article-search';
import { useScreening } from '@/composables/use-screening';
import { useToast } from '../composables/use-toast';
import { useFullTextAttachment } from '@/composables/use-full-text-attachment';
import type { NetworkExportFormat } from '../utils/network-export';
import type { CocitationNode, CocitationEdge } from '../types/biblio-cocitation';

const toast = useToast();

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

/**
 * Article detail panel (opened via "open linked record" from co-citation
 * detail). Mirrors citation-network pattern.
 */
const {
  selectedArticle: detailArticle,
  auditTrail: detailAuditTrail,
  selectArticle,
  refreshArticle,
  updateNotes,
  updateTags,
  updateLabels,
  updateCriteria,
  updateMetadata,
  moveArticle,
  attachFullText,
  deleteFullTextAttachment,
} = useArticleSearch();
const { screenArticle } = useScreening();
const showArticleDetail = ref(false);
const isArticleDetailFullScreen = ref(false);
// Full-text attach UI orchestration is centralized in `useFullTextAttachment`
// (shared with the other detail-panel host views).
const { handleAttachFullText } = useFullTextAttachment({ attachFullText });

/** Open the full article detail panel from the co-citation detail panel. */
async function onOpenLinkedRecord(articleId: string): Promise<void> {
  try {
    await selectArticle(articleId);
    showArticleDetail.value = true;
  } catch {
    toast.show('Failed to load article details', 'error');
  }
}

/** Close the article detail panel. */
function onCloseArticleDetail(): void {
  showArticleDetail.value = false;
  isArticleDetailFullScreen.value = false;
  detailArticle.value = null;
  detailAuditTrail.value = [];
}

const {
  graphRef,
  focusedNodeId,
  visibleNodeCount,
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
  exportPrefix: 'cocitation-network',
  graphType: 'undirected',
  recalculateIterations: 150,
});

// Control state (co-citation-specific)
const scope = ref<'included' | 'all'>('included');
const normalization = ref<'raw' | 'cosine' | 'jaccard' | 'pearson'>('cosine');
const minCitationCount = ref(2);
const minCoCitation = ref(2);
/**
 * Hide nodes whose matched article has status 'rejected'. Client-side only
 * (toggles graphology `hidden` attribute).
 */
const hideRejectedMatches = ref(false);
const selectedPaper = ref<CocitationNode | null>(null);
const showHeatmap = ref(false);

const stats = computed(() => ({
  totalNodes: nodeCount.value,
  totalEdges: edgeCount.value,
  visibleNodes: visibleNodeCount.value || nodeCount.value,
  clusterCount: clusterCount.value,
}));

/** Paper search entries for autocomplete search. */
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
      matchedArticleStatus: attrs.matchedArticleStatus,
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
  focusNode(nodeId);
  if (!nodeId) {
    selectedPaper.value = null;
    return;
  }
  selectedPaper.value = getNode(nodeId);
}

function onNavigateToPaper(nodeId: string) {
  onNodeClick(nodeId);
  graphRef.value?.locateNode(nodeId);
}

/**
 * Handle the sidebar autocomplete "locate paper" event: focus + pan/zoom to
 * the node AND open the detail panel. `locateByLabel` returns the node id when
 * a match is found; we feed it into the same node-selection path as a direct
 * click so the panel context is consistent.
 */
function onLocatePaper(label: string) {
  const nodeId = locateByLabel(label);
  if (nodeId) {
    selectedPaper.value = getNode(nodeId);
  }
}

async function onParamsChange() {
  selectedPaper.value = null;
  focusNode(null);
  selectedClusters.value = [];
  await doFetch();
  if (graph.value) {
    await applyLayout(graph.value, 100, layoutMode.value);
    visibleNodeCount.value = nodeCount.value;
    recalculateTrigger.value++;
  }
}

/** Last search string emitted by the controls sidebar, so the
 * "Hide rejected matches" toggle can re-compose search + rejected filters. */
let lastSearch = '';

function onFilterChange(filters: { search: string }) {
  if (!graph.value) return;
  lastSearch = filters.search;
  /* Co-citation nodes lack `weight` (carry coCitationCount/citationCount),
   * so keyword filter can't be reused. Use dedicated co-citation search.
   * The rejected-matches filter layers on top and never un-hides. */
  applyCocitationGraphFilters(graph.value, { search: filters.search });
  visibleNodeCount.value = applyRejectedMatchesFilter(graph.value, hideRejectedMatches.value);
}

/**
 * React to the "Hide rejected matches" toggle. Re-applies the search filter
 * (which recomputes hidden-by-search) then layers the rejected filter on top.
 */
function onHideRejectedToggle(value: boolean): void {
  hideRejectedMatches.value = value;
  if (!graph.value) return;
  // Reset all hidden flags so the search filter + rejected filter compose
  // cleanly from a known state.
  graph.value.forEachNode((id) => {
    graph.value!.setNodeAttribute(id, 'hidden', false);
  });
  // Re-run the search filter (last emitted search) then the rejected filter.
  applyCocitationGraphFilters(graph.value, { search: lastSearch });
  visibleNodeCount.value = applyRejectedMatchesFilter(graph.value, hideRejectedMatches.value);
}

async function onExportImage(format: NetworkExportFormat) {
  const renderer = (graphRef.value as { renderer?: unknown } | null)?.renderer;
  await exportImage(format, (renderer as Parameters<typeof exportImage>[1]) ?? null);
}

async function onResetAnalysis() {
  resetViewState();
  scope.value = 'included';
  normalization.value = 'cosine';
  minCitationCount.value = 2;
  minCoCitation.value = 2;
  selectedPaper.value = null;
  await onParamsChange();
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
          :hide-rejected-matches="hideRejectedMatches"
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
          @update:hide-rejected-matches="onHideRejectedToggle"
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

    <!-- Central Content Area (hidden when article detail is fullscreen) -->
    <div
      v-show="!(showArticleDetail && isArticleDetailFullScreen)"
      class="flex-1 flex flex-col min-h-0 relative"
    >
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

        <!-- Scope + heatmap toggles (top-right overlay) -->
        <div class="absolute top-3 right-3 z-10 flex items-center gap-2">
          <!-- Scope diagnostic badge: surfaces the in-scope article count so the
               scope toggle's effect is visible even when the graph looks similar. -->
          <span
            v-if="meta"
            class="hidden sm:inline-flex items-center gap-1 px-2.5 py-1.5 text-[11px] font-medium bg-white border border-slate-200 rounded-lg shadow-sm"
            :title="`Scope: ${scope === 'included' ? 'Included' : 'All non-duplicate'} articles`"
          >
            <span class="material-symbols-outlined text-sm text-slate-400">filter_list</span>
            <span class="text-slate-700">{{ scope === 'included' ? 'Included' : 'All' }}</span>
            <span class="text-slate-400">({{ meta.inScopeArticleCount }})</span>
          </span>
          <button
            v-if="graph && nodeCount > 0"
            class="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-slate-600 bg-white border border-slate-200 rounded-lg shadow-sm hover:bg-slate-50 transition-colors"
            @click="showHeatmap = !showHeatmap"
          >
            <span class="material-symbols-outlined text-sm">{{
              showHeatmap ? 'grid_off' : 'grid_on'
            }}</span>
            {{ showHeatmap ? 'Hide' : 'Heatmap' }}
          </button>
        </div>

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

    <!-- Detail panel (hidden while the article detail overlay is open) -->
    <Transition name="detail-slide">
      <CocitationDetailPanel
        v-if="selectedPaper && !showArticleDetail"
        :paper="selectedPaper"
        :co-cited-papers="coCitedPapers"
        class="w-72 shrink-0"
        @close="onNodeClick(null)"
        @navigate-paper="onNavigateToPaper"
        @open-linked-record="onOpenLinkedRecord"
      />
    </Transition>

    <!-- Full article detail panel (opened from the co-citation detail panel). -->
    <Transition name="detail-slide">
      <ArticleDetailPanel
        v-if="showArticleDetail && detailArticle"
        :article="detailArticle"
        :audit-trail="detailAuditTrail"
        :has-previous="false"
        :has-next="false"
        :has-return-target="false"
        :full-screen="isArticleDetailFullScreen"
        :article-position="1"
        :article-total="1"
        @close="onCloseArticleDetail"
        @toggle-full-screen="isArticleDetailFullScreen = !isArticleDetailFullScreen"
        @update-notes="updateNotes"
        @update-tags="updateTags"
        @update-labels="updateLabels"
        @update-criteria="updateCriteria"
        @update-metadata="updateMetadata"
        @screen-article="screenArticle"
        @move-article="moveArticle"
        @attach-full-text="handleAttachFullText"
        @delete-full-text="deleteFullTextAttachment"
        @refresh-article="refreshArticle"
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
