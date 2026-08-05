import { ref, computed, type Ref } from 'vue';
import Graph from 'graphology';
import { useNetworkLayout } from './use-network-layout';
import { exportNetworkPng, exportNetworkGexf } from '../utils/network-export';
import type { NetworkExportFormat } from '../utils/network-export';

/**
 * Minimal structural type for a network graph component ref.
 */
export interface NetworkGraphHandle {
  locateNode: (nodeId: string) => void;
  resetZoom: () => void;
  refresh: () => void;
}

/** Configuration for {@link useNetworkView}. */
export interface NetworkViewOptions {
  /** The reactive graph ref. */
  graph: Ref<Graph | null>;
  /** Node attribute holding the publication year. Co-author/keyword: `'avgYear'`; citation/cocitation: `'year'`. */
  yearAttribute?: 'year' | 'avgYear';
  /** Graph type for subgraph build. Citation networks: `'directed'`. */
  graphType?: 'directed' | 'undirected';
  /** Filename prefix for exports, e.g. `'coauthor-network'`. */
  exportPrefix: string;
  /** Fallback year range when the graph has no year data. */
  defaultYearRange?: { min: number; max: number };
  /** ForceAtlas2 iteration count for `onRecalculate`. */
  recalculateIterations?: number;
}

/**
 * Shared view-state for the four bibliometric network views.
 *
 * Owns cross-cutting state (focus, counts, color/layout modes, cluster
 * selection, sidebar) and identical handlers (cluster toggle, layout mode
 * switch, image export, subgraph recalculation).
 */
export function useNetworkView(options: NetworkViewOptions) {
  const {
    graph,
    yearAttribute = 'year',
    graphType = 'undirected',
    exportPrefix,
    defaultYearRange = { min: 2000, max: 2024 },
    recalculateIterations = 100,
  } = options;

  // Layout composable - instance-scoped (fresh state per view).
  const { isLayouting, applyLayout } = useNetworkLayout();

  // ── Refs exposed to the template ────────────────────────────────
  const graphRef = ref<NetworkGraphHandle | null>(null);
  const focusedNodeId = ref<string | null>(null);
  const visibleNodeCount = ref(0);
  const visibleEdgeCount = ref(0);
  const colorMode = ref<'cluster' | 'temporal'>('cluster');
  const layoutMode = ref<'fixed' | 'dynamic'>('fixed');
  const selectedClusters = ref<number[]>([]);
  const sidebarCollapsed = ref(false);
  const recalculateTrigger = ref(0);

  // ── Graph-component handle proxies ──────────────────────────────
  function locateNode(nodeId: string): void {
    graphRef.value?.locateNode(nodeId);
  }
  function resetZoom(): void {
    graphRef.value?.resetZoom();
  }
  function refresh(): void {
    graphRef.value?.refresh();
  }

  // ── Computed: cluster count (visible nodes only) ────────────────
  /** Number of distinct clusters among visible (non-hidden) nodes. */
  const clusterCount = computed(() => {
    // Touch reactive deps so the computed re-evaluates after recalculate.
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

  // ── Computed: year range for temporal color gradient ────────────
  /** Min/max year across all nodes, for the temporal color gradient. */
  const yearRange = computed(() => {
    if (!graph.value) return { ...defaultYearRange };
    let min = Infinity;
    let max = -Infinity;
    graph.value.forEachNode((node) => {
      const yr = graph.value!.getNodeAttribute(node, yearAttribute) as number | null;
      if (yr !== null && yr !== undefined) {
        if (yr < min) min = yr;
        if (yr > max) max = yr;
      }
    });
    if (min === Infinity || max === -Infinity) {
      return { ...defaultYearRange };
    }
    if (min === max) {
      return { min: min - 1, max: min + 1 };
    }
    return { min: Math.floor(min), max: Math.ceil(max) };
  });

  // ── Selection focus ─────────────────────────────────────────────
  /** Set or clear the focused node. Call with `null` to clear. */
  function focusNode(nodeId: string | null): void {
    focusedNodeId.value = nodeId;
  }

  /** Navigate to a node: focus it and pan/zoom the camera onto it. */
  function navigateToNode(nodeId: string): void {
    focusNode(nodeId);
    locateNode(nodeId);
  }

  /**
   * Find a node by its `label` attribute and focus + locate it.
   * Used by the autocomplete "locate" handlers in each view.
   * Returns the node id if found, or `null` if no match.
   */
  function locateByLabel(label: string): string | null {
    if (!graph.value) return null;
    const nodeId = graph.value.findNode(
      (node) => (graph.value!.getNodeAttribute(node, 'label') as string) === label
    );
    if (nodeId) {
      focusNode(nodeId);
      locateNode(nodeId);
      return nodeId;
    }
    return null;
  }

  // ── Cluster selection ───────────────────────────────────────────
  /** Toggle a cluster id in/out of the selected set. */
  function onSelectCluster(clusterId: number): void {
    const idx = selectedClusters.value.indexOf(clusterId);
    if (idx >= 0) {
      selectedClusters.value.splice(idx, 1);
    } else {
      selectedClusters.value.push(clusterId);
    }
  }

  /** Clear all selected clusters. */
  function onClearClusters(): void {
    selectedClusters.value = [];
  }

  // ── Layout mode ─────────────────────────────────────────────────
  /** Switch layout mode and trigger a full recalculate. */
  async function onLayoutModeChange(mode: 'fixed' | 'dynamic'): Promise<void> {
    layoutMode.value = mode;
    if (graph.value) {
      await onRecalculate();
    }
  }

  // ── Image export ────────────────────────────────────────────────
  /**
   * Export the network as PNG (via renderer) or GEXF (via graph).
   */
  async function onExportImage(
    format: NetworkExportFormat,
    renderer?: Parameters<typeof exportNetworkPng>[0] | null
  ): Promise<void> {
    try {
      if (format === 'png') {
        if (!renderer) return;
        await exportNetworkPng(renderer, `${exportPrefix}.png`);
      } else if (format === 'gexf') {
        if (!graph.value) return;
        await exportNetworkGexf(graph.value, `${exportPrefix}.gexf`);
      }
    } catch (err) {
      console.error(`[export] ${exportPrefix} export failed:`, err);
    }
  }

  // ── Recalculate (subgraph layout) ───────────────────────────────
  /**
   * Build a temporary subgraph of visible (non-hidden) nodes + edges,
   * re-run Louvain + ForceAtlas2 on it, and write the new coordinates and
   * cluster assignments back to the parent graph. Then refresh the renderer
   * and bump `recalculateTrigger` so computeds re-evaluate.
   */
  async function onRecalculate(): Promise<void> {
    if (!graph.value) return;

    isLayouting.value = true;
    try {
      const sub = new Graph({ type: graphType, multi: false });

      graph.value.forEachNode((node, attrs) => {
        if (attrs.hidden !== true) {
          sub.addNode(node, { ...attrs });
        }
      });

      graph.value.forEachEdge((edge, attrs, source, target) => {
        if (attrs.hidden !== true && sub.hasNode(source) && sub.hasNode(target)) {
          if (graphType === 'directed') {
            sub.addDirectedEdgeWithKey(edge, source, target, { ...attrs });
          } else {
            sub.addUndirectedEdgeWithKey(edge, source, target, { ...attrs });
          }
        }
      });

      if (sub.order === 0) return;

      await applyLayout(sub, recalculateIterations, layoutMode.value);

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

  // ── Reset (shared subset) ───────────────────────────────────────
  /**
   * Reset the cross-cutting view state shared by all four views. Each view's
   * `onResetAnalysis` calls this, then applies its own view-specific resets
   * (e.g. re-fetching with default params, clearing the counting mode).
   */
  function resetViewState(): void {
    colorMode.value = 'cluster';
    layoutMode.value = 'fixed';
    selectedClusters.value = [];
    focusNode(null);
  }

  return {
    // Layout composable passthrough
    isLayouting,
    applyLayout,
    // Refs
    graphRef,
    focusedNodeId,
    visibleNodeCount,
    visibleEdgeCount,
    colorMode,
    layoutMode,
    selectedClusters,
    sidebarCollapsed,
    recalculateTrigger,
    // Computed
    clusterCount,
    yearRange,
    // Graph handle proxies
    locateNode,
    resetZoom,
    refresh,
    // Selection
    focusNode,
    navigateToNode,
    locateByLabel,
    // Cluster
    onSelectCluster,
    onClearClusters,
    // Layout
    onLayoutModeChange,
    onRecalculate,
    // Export
    onExportImage,
    // Reset
    resetViewState,
  };
}
