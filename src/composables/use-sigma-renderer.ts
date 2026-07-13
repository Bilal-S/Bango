import { ref, shallowRef, onUnmounted } from 'vue';
import Sigma from 'sigma';
import { createEdgeArrowProgram, NodeCircleProgram } from 'sigma/rendering';
import type Graph from 'graphology';
import {
  applyGraphFilters as applyGraphFiltersImpl,
  applyCitationGraphFilters as applyCitationGraphFiltersImpl,
  applyKeywordGraphFilters as applyKeywordGraphFiltersImpl,
} from '../utils/graph-filters';

class NodeBorderProgram extends NodeCircleProgram {
  getDefinition() {
    const definition = super.getDefinition();
    definition.FRAGMENT_SHADER_SOURCE = `
precision highp float;

varying vec4 v_color;
varying vec2 v_diffVector;
varying float v_radius;

uniform float u_correctionRatio;
uniform float u_sizeRatio;

const vec4 transparent = vec4(0.0, 0.0, 0.0, 0.0);

void main(void) {
  float border = u_correctionRatio * 2.0;
  float dist = length(v_diffVector) - v_radius + border;

  #ifdef PICKING_MODE
  if (dist > border)
    gl_FragColor = transparent;
  else
    gl_FragColor = v_color;

  #else
  float t = 0.0;
  if (dist > border)
    t = 1.0;
  else if (dist > 0.0)
    t = dist / border;

  // 1 pixel in the local coordinate space corresponds to 2.0 * u_correctionRatio / u_sizeRatio
  float pixel_unit = 2.0 * u_correctionRatio / u_sizeRatio;

  float outerBorder = pixel_unit * 1.5;
  float innerGap = pixel_unit * 1.5;
  float minCenterRadius = pixel_unit * 1.5;

  if (v_radius < outerBorder + innerGap + minCenterRadius) {
    float scale = v_radius / (outerBorder + innerGap + minCenterRadius);
    outerBorder *= scale;
    innerGap *= scale;
  }

  float r = length(v_diffVector);
  vec4 finalColor = v_color;

  if (v_radius - r < outerBorder) {
    // Outer border color: #0f172a (slate-900)
    finalColor = vec4(0.059, 0.090, 0.165, v_color.a);
  } else if (v_radius - r < outerBorder + innerGap) {
    // Inner gap color: white #ffffff
    finalColor = vec4(1.0, 1.0, 1.0, v_color.a);
  }

  gl_FragColor = mix(finalColor, transparent, t);
  #endif
}
    `;
    return definition;
  }
}

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
  /** Label render size threshold - labels hidden below this zoom level. Default 1.5 */
  labelRenderSizeThreshold?: number;
  /**
   * Custom arrow-head dimensions for directed edges. The arrow program scales
   * relative to the edge's `size` attribute. Increasing these ratios produces
   * larger arrowheads. Sigma defaults are length 2.5 / wideness 2.
   */
  edgeArrowSize?: { length?: number; wideness?: number };
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
  // IMPORTANT: this ref MUST live inside the function (component instance scope),
  // NOT at module scope. A module-scoped singleton is shared across all callers,
  // so one component's onUnmounted → destroyRenderer() would kill another
  // component's renderer, causing crashes during route transitions where parent
  // and child both use this composable.
  const renderer = shallowRef<Sigma | null>(null);
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
      nodeProgramClasses: {
        circle: NodeCircleProgram,
        included: NodeBorderProgram,
      },
    };

    // When a custom arrow size is requested, build a custom arrow program with
    // enlarged length/wideness ratios and register it for the `arrow` edge type.
    if (options.edgeArrowSize) {
      settings.edgeProgramClasses = {
        arrow: createEdgeArrowProgram({
          lengthToThicknessRatio: options.edgeArrowSize.length ?? 2.5,
          widenessToThicknessRatio: options.edgeArrowSize.wideness ?? 2,
        }),
      };
    }

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
   * Apply visibility filters to graph nodes based on minPapers, minLinkStrength,
   * maxAuthors, and a search query. Delegates to the pure implementation in
   * `utils/graph-filters.ts` (unit-tested there without DOM/Sigma scaffolding).
   */
  function applyGraphFilters(
    g: Graph,
    filters: { minPapers: number; minLinkStrength: number; maxAuthors: number; search: string }
  ): { visibleNodes: number; visibleEdges: number } {
    return applyGraphFiltersImpl(g, filters);
  }

  /**
   * Apply visibility filters to citation graph nodes. Delegates to the pure
   * implementation in `utils/graph-filters.ts`.
   */
  function applyCitationGraphFilters(
    g: Graph,
    filters: {
      minCitations: number;
      showIsolated: boolean;
      search: string;
      yearRange?: [number, number] | null;
    }
  ): { visibleNodes: number; visibleEdges: number } {
    return applyCitationGraphFiltersImpl(g, filters);
  }

  /**
   * Apply visibility filters to keyword graph nodes. Delegates to the pure
   * implementation in `utils/graph-filters.ts`.
   */
  function applyKeywordGraphFilters(
    g: Graph,
    filters: { minOccurrences: number; minCooccurrence: number; search: string }
  ): { visibleNodes: number; visibleEdges: number } {
    return applyKeywordGraphFiltersImpl(g, filters);
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
    applyKeywordGraphFilters,
    refresh,
  };
}
