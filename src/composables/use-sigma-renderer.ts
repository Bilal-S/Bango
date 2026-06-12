import { ref, onUnmounted } from 'vue';
import Sigma from 'sigma';
import type Graph from 'graphology';

const renderer = ref<Sigma | null>(null);

export interface SigmaRendererOptions {
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
   * Export the current graph view as a PNG data URL.
   */
  function exportImage(): string | null {
    if (!renderer.value) return null;
    // Sigma v3 doesn't expose toDataURL directly — grab it from the WebGL canvas
    const canvases = renderer.value.getCanvases?.();
    if (canvases) {
      // Sigma v3.0.4+
      const canvas = canvases.edges ?? canvases.nodes;
      if (canvas) return canvas.toDataURL('image/png');
    }
    // Fallback: query the DOM for the sigma canvas element
    const container = renderer.value.getContainer?.() ?? renderer.value.getGraph();
    if (!container) return null;
    const el = (container as unknown as HTMLElement).querySelector?.(
      'canvas'
    ) as HTMLCanvasElement | null;
    return el?.toDataURL('image/png') ?? null;
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
    graph: Graph,
    filters: { minPapers: number; minLinkStrength: number; search: string }
  ): { visibleNodes: number; visibleEdges: number } {
    const { minPapers, minLinkStrength, search } = filters;
    const searchLower = search.toLowerCase();

    // First, determine which nodes pass the filter
    const nodeVisible = new Map<string, boolean>();
    for (const node of graph.nodes()) {
      const weight = graph.getNodeAttribute(node, 'weight') as number;
      const label = (graph.getNodeAttribute(node, 'label') as string) ?? '';
      const passesPapers = weight >= minPapers;
      const passesSearch = !searchLower || label.toLowerCase().includes(searchLower);
      const visible = passesPapers && passesSearch;
      nodeVisible.set(node, visible);
      graph.setNodeAttribute(node, 'hidden', !visible);
    }

    // Then, determine which edges pass
    let visibleEdges = 0;
    for (const edge of graph.edges()) {
      const weight = graph.getEdgeAttribute(edge, 'weight') as number;
      const source = graph.source(edge);
      const target = graph.target(edge);
      const passesStrength = weight >= minLinkStrength;
      const bothEndsVisible = nodeVisible.get(source) === true && nodeVisible.get(target) === true;
      const visible = passesStrength && bothEndsVisible;
      graph.setEdgeAttribute(edge, 'hidden', !visible);
      if (visible) visibleEdges++;
    }

    const visibleNodes = graph.nodes().filter((n) => nodeVisible.get(n) === true).length;

    return { visibleNodes, visibleEdges };
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
    exportImage,
    locateNode,
    applyGraphFilters,
  };
}
