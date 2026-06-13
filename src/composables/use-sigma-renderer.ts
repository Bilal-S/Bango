import { ref, shallowRef, onUnmounted } from 'vue';
import Sigma from 'sigma';
import type Graph from 'graphology';

const renderer = shallowRef<Sigma | null>(null);

interface SigmaRendererOptions {
  /** Min camera ratio (zoom in limit). Default 0.1 */
  minCameraRatio?: number;
  /** Max camera ratio (zoom out limit). Default 10 */
  maxCameraRatio?: number;
  /** Render edge labels on hover. Default true */
  renderEdgeLabels?: boolean;
  /** Default node size in pixels. Default 10 */
  defaultNodeSize?: number;
  /** Default edge color. Default '#e2e8f0' */
  defaultEdgeColor?: string;
  /** Label render size threshold — labels hidden below this zoom level. Default 1.5 */
  labelRenderSizeThreshold?: number;
}

/**
 * Composable for managing a Sigma.js v3 renderer lifecycle.
 *
 * Usage:
 *   const container = ref<HTMLElement>();
 *   const { initRenderer, destroyRenderer, renderer } = useSigmaRenderer();
 *   await initRenderer(container.value!, graph, opts);
 */
export function useSigmaRenderer() {
  const isRendering = ref(false);

  /**
   * Create and mount a Sigma renderer onto the given DOM container.
   */
  function initRenderer(
    container: HTMLElement,
    graph: Graph,
    options: SigmaRendererOptions = {}
  ): Sigma {
    // Destroy any existing renderer first
    destroyRenderer();

    const settings: Partial<Sigma['settings']> = {
      renderEdgeLabels: options.renderEdgeLabels ?? true,
      defaultEdgeColor: options.defaultEdgeColor ?? '#e2e8f0',
      labelRenderSizeThreshold: options.labelRenderSizeThreshold ?? 1.5,
      stagePadding: 30,
    };

    const sig = new Sigma(graph, container, settings);

    // Set camera zoom limits
    const camera = sig.getCamera();
    camera.minRatio = options.minCameraRatio ?? 0.1;
    camera.maxRatio = options.maxCameraRatio ?? 10;

    renderer.value = sig;
    isRendering.value = true;

    return sig;
  }

  /**
   * Destroy the current Sigma renderer and free resources.
   */
  function destroyRenderer(): void {
    if (renderer.value) {
      renderer.value.kill();
      renderer.value = null;
    }
    isRendering.value = false;
  }

  /**
   * Reset camera to default view (fit all nodes).
   */
  function resetZoom(): void {
    if (!renderer.value) return;
    const camera = renderer.value.getCamera();
    camera.animate({ ratio: 1, x: 0.5, y: 0.5 }, { duration: 300 });
  }

  /**
   * Pan and zoom the camera to center on a specific node.
   */
  function locateNode(nodeId: string): void {
    if (!renderer.value || !renderer.value.getGraph().hasNode(nodeId)) return;
    const nodeDisplayData = renderer.value.getNodeDisplayData(nodeId);
    if (!nodeDisplayData) return;
    const camera = renderer.value.getCamera();
    camera.animate({ x: nodeDisplayData.x, y: nodeDisplayData.y, ratio: 0.5 }, { duration: 400 });
  }

  /**
   * Apply visibility filters to graph nodes based on minPapers, minLinkStrength, and search query.
   * Returns { visibleNodes, visibleEdges } counts.
   */
  function applyGraphFilters(
    g: Graph,
    filters: { minPapers: number; minLinkStrength: number; maxAuthors: number; search: string }
  ): { visibleNodes: number; visibleEdges: number } {
    const { minPapers, minLinkStrength, maxAuthors, search } = filters;
    const searchLower = search.toLowerCase();

    // First, determine which edges pass the maxAuthors filter.
    // An edge whose maxAuthorCount exceeds the threshold means it comes from
    // a mega-author paper, so we drop that edge.
    const edgeVisible = new Map<string, boolean>();
    for (const edge of g.edges()) {
      const mac = (g.getEdgeAttribute(edge, 'maxAuthorCount') as number) ?? 0;
      edgeVisible.set(edge, mac <= maxAuthors);
    }

    // Determine which nodes pass the filter
    const nodeVisible = new Map<string, boolean>();
    for (const node of g.nodes()) {
      const weight = g.getNodeAttribute(node, 'weight') as number;
      const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
      const passesPapers = weight >= minPapers;
      const passesSearch = !searchLower || label.toLowerCase().includes(searchLower);
      const visible = passesPapers && passesSearch;
      nodeVisible.set(node, visible);
      g.setNodeAttribute(node, 'hidden', !visible);
    }

    // Then, determine which edges pass all filters
    let visibleEdges = 0;
    for (const edge of g.edges()) {
      const weight = g.getEdgeAttribute(edge, 'weight') as number;
      const source = g.source(edge);
      const target = g.target(edge);
      const passesStrength = weight >= minLinkStrength;
      const passesMaxAuthors = edgeVisible.get(edge) !== false;
      const bothEndsVisible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
      const visible = passesStrength && passesMaxAuthors && bothEndsVisible;
      g.setEdgeAttribute(edge, 'hidden', !visible);
      if (visible) visibleEdges++;
    }

    const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

    return { visibleNodes, visibleEdges };
  }

  /**
   * Apply visibility filters to citation graph nodes based on minCitations,
   * showIsolated, and search query.
   *
   * - `minCitations`: hide papers with fewer than N incoming citations.
   * - `showIsolated`: when false, hide nodes with zero degree (no edges).
   * - `search`: case-insensitive substring match on label/title/authors.
   *
   * Returns { visibleNodes, visibleEdges } counts.
   */
  function applyCitationGraphFilters(
    g: Graph,
    filters: { minCitations: number; showIsolated: boolean; search: string }
  ): { visibleNodes: number; visibleEdges: number } {
    const { minCitations, showIsolated, search } = filters;
    const searchLower = search.toLowerCase();

    // Determine which nodes pass the filter
    const nodeVisible = new Map<string, boolean>();
    for (const node of g.nodes()) {
      const numCited = (g.getNodeAttribute(node, 'numCited') as number) ?? 0;
      const label = (g.getNodeAttribute(node, 'label') as string) ?? '';
      const title = (g.getNodeAttribute(node, 'title') as string) ?? '';
      const authors = (g.getNodeAttribute(node, 'authors') as string) ?? '';
      const degree = g.degree(node);

      const passesCitations = numCited >= minCitations;
      const passesIsolated = showIsolated || degree > 0;
      const passesSearch =
        !searchLower ||
        label.toLowerCase().includes(searchLower) ||
        title.toLowerCase().includes(searchLower) ||
        authors.toLowerCase().includes(searchLower);
      const visible = passesCitations && passesIsolated && passesSearch;
      nodeVisible.set(node, visible);
      g.setNodeAttribute(node, 'hidden', !visible);
    }

    // Edges are visible only if both endpoints are visible
    let visibleEdges = 0;
    for (const edge of g.edges()) {
      const source = g.source(edge);
      const target = g.target(edge);
      const visible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
      g.setEdgeAttribute(edge, 'hidden', !visible);
      if (visible) visibleEdges++;
    }

    const visibleNodes = g.nodes().filter((n: string) => nodeVisible.get(n) === true).length;

    return { visibleNodes, visibleEdges };
  }

  /**
   * Force the Sigma renderer to re-read graph attributes and redraw.
   */
  function refresh(): void {
    renderer.value?.refresh();
  }

  // Auto-cleanup when the hosting component unmounts
  onUnmounted(() => {
    destroyRenderer();
  });

  return {
    renderer,
    isRendering,
    initRenderer,
    destroyRenderer,
    resetZoom,
    locateNode,
    applyGraphFilters,
    applyCitationGraphFilters,
    refresh,
  };
}
