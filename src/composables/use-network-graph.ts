import { ref, computed, shallowRef, useTemplateRef, watch, onUnmounted } from 'vue';
import type Graph from 'graphology';
import { useSigmaRenderer } from './use-sigma-renderer';
import type { SigmaRendererOptions } from './use-sigma-renderer';
import type { NetworkGraphProps } from '../types/network-graph';

/** Graphology node-attribute bag (`getNodeAttributes` return type). */
type NodeAttrs = ReturnType<Graph['getNodeAttributes']>;

/** Options for {@link useNetworkGraph}. */
export interface NetworkGraphOptions<THover> {
  /** Sigma renderer settings forwarded to `initRenderer` (per-graph tuning). */
  rendererOptions: SigmaRendererOptions;
  /**
   * Build the hover-tooltip payload for a node from its graph attributes.
   * Called on every `enterNode` renderer event.
   */
  mapHoveredNode: (node: string, attrs: NodeAttrs) => THover;
  /** Domain visual-state pass (colors, sizes, dimming) over the graph. */
  applyVisualState: () => void;
  /** Forwards renderer node/stage clicks (the component re-emits `node-click`). */
  onNodeClick: (nodeId: string | null) => void;
  /**
   * Install the shared reapply watchers (focusedNodeId, colorMode,
   * selectedClusters, recalculateTrigger - all calling `applyVisualState`).
   * Default `true`. Pass `false` when the component dispatches its own
   * visual-state logic per prop change (the co-author graph's focus/cluster/
   * clear priority dispatch).
   */
  installStandardWatchers?: boolean;
  /** Called synchronously when a non-null graph arrives (before the rAF). */
  onBeforeInit?: () => void;
  /**
   * Called after the renderer initialized on a new graph, instead of the
   * default `applyVisualState()` call.
   */
  onGraphReady?: () => void;
}

/**
 * Shared scaffolding for the four bibliometric graph components.
 *
 * Owns the Sigma renderer lifecycle (requestAnimationFrame-deferred init,
 * unmount guards), the hover tooltip state, the sigma event bindings, and
 * the standard visual-state reapply watchers. The domain component keeps
 * only its tooltip template, its `applyVisualState`/`getNodeColor`
 * implementations, and any domain-specific watchers.
 */
export function useNetworkGraph<THover>(
  props: NetworkGraphProps,
  options: NetworkGraphOptions<THover>
) {
  /** Sigma container element; bound in the component template via `ref="sigmaContainer"`. */
  const containerRef = useTemplateRef<HTMLElement>('sigmaContainer');
  const hoveredNode = shallowRef<THover | null>(null);
  const tooltipX = ref(0);
  const tooltipY = ref(0);

  /* Guard against async callbacks (rAF) firing after unmount. Without this,
     a pending rAF can call initRenderer() on a detached container during
     route transitions, causing crashes. */
  let isUnmounted = false;
  let pendingFrame: number | null = null;

  const { renderer, initRenderer, destroyRenderer, locateNode, resetZoom, refresh } =
    useSigmaRenderer();

  const hasGraph = computed(() => (props.graph?.order ?? 0) > 0);

  const tooltipPosition = computed(() => ({
    left: `${tooltipX.value + 12}px`,
    top: `${tooltipY.value - 8}px`,
  }));

  watch(
    () => props.graph,
    (g) => {
      if (pendingFrame !== null) {
        cancelAnimationFrame(pendingFrame);
        pendingFrame = null;
      }
      if (!g) {
        destroyRenderer();
        return;
      }
      if (!containerRef.value) return;
      options.onBeforeInit?.();
      pendingFrame = requestAnimationFrame(() => {
        pendingFrame = null;
        // Abort if the component was unmounted while we waited for the frame.
        // This prevents mounting a Sigma renderer onto a detached DOM node.
        if (isUnmounted || !containerRef.value || !g) return;
        initRenderer(containerRef.value, g, options.rendererOptions);
        bindSigmaEvents();
        if (options.onGraphReady) {
          options.onGraphReady();
        } else {
          options.applyVisualState();
        }
      });
    }
  );

  if (options.installStandardWatchers !== false) {
    watch(
      () => props.focusedNodeId,
      () => {
        options.applyVisualState();
      }
    );

    watch(
      () => props.colorMode,
      () => {
        options.applyVisualState();
      }
    );

    watch(
      () => props.selectedClusters,
      () => {
        options.applyVisualState();
      },
      { deep: true }
    );

    watch(
      () => props.recalculateTrigger,
      () => {
        if (props.graph) options.applyVisualState();
      }
    );
  }

  function bindSigmaEvents() {
    if (!renderer.value) return;
    const sig = renderer.value;

    sig.on('enterNode', ({ node }) => {
      if (!props.graph) return;
      const attrs = props.graph.getNodeAttributes(node);
      hoveredNode.value = options.mapHoveredNode(node, attrs);
    });

    sig.on('leaveNode', () => {
      hoveredNode.value = null;
    });

    sig.on('moveBody', (payload) => {
      const mouseEvt = payload.event.original as MouseEvent;
      if (!mouseEvt.x) return;
      const rect = containerRef.value?.getBoundingClientRect();
      if (rect) {
        tooltipX.value = mouseEvt.x - rect.left;
        tooltipY.value = mouseEvt.y - rect.top;
      }
    });

    sig.on('clickNode', ({ node }) => {
      options.onNodeClick(node);
    });

    sig.on('clickStage', () => {
      options.onNodeClick(null);
    });
  }

  onUnmounted(() => {
    isUnmounted = true;
    if (pendingFrame !== null) {
      cancelAnimationFrame(pendingFrame);
      pendingFrame = null;
    }
    destroyRenderer();
  });

  return {
    hoveredNode,
    hasGraph,
    tooltipPosition,
    renderer,
    locateNode,
    resetZoom,
    refresh,
  };
}
