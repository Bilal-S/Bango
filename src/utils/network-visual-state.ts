import type Graph from 'graphology';

/** Per-domain config for the shared focus/cluster visual-state pass. */
export interface VisualStateConfig {
  /** Base node color (domain: cluster palette / temporal gradient). */
  getNodeColor: (nodeId: string) => string;
  /**
   * Base node size before dimming. Bounds are over node `weight` values
   * (min floored at 1, max at least 1); flat ranges are the domain's call.
   */
  getNodeSize: (nodeId: string, minNodeWeight: number, maxNodeWeight: number) => number;
  /** Highlight edge color while focus/cluster is active. Default slate-400. */
  highlightEdgeColor?: string;
  /** Idle edge color (no focus/cluster active). Default slate-300. */
  idleEdgeColor?: string;
  /** Optional edge-thickness pass (bounds over edge `weight`). */
  computeEdgeSize?: (edge: string, minEdgeWeight: number, maxEdgeWeight: number) => number;
}

/** Visual-state inputs (the shared graph props that drive the pass). */
export interface VisualStateInput {
  focusedNodeId: string | null;
  selectedClusters: number[];
}

/**
 * Shared focus/cluster visual-state pass for the bibliometric graphs.
 *
 * Priority: focusedNodeId > selectedClusters > clear. Dimmed nodes get the
 * base color at 15% alpha (`${color}26`) and 0.6x their base size; dimmed
 * edges go slate-100, active edges highlight, idle edges use
 * `idleEdgeColor`. Replaces the per-component copies that previously lived
 * in the co-author, keyword, and co-citation graph components.
 */
export function applyFocusClusterVisualState(
  g: Graph,
  input: VisualStateInput,
  cfg: VisualStateConfig
): void {
  const isFocusActive = !!input.focusedNodeId;
  const isClusterActive = input.selectedClusters.length > 0;

  let focusNeighborsSet = new Set<string>();
  if (input.focusedNodeId && g.hasNode(input.focusedNodeId)) {
    focusNeighborsSet = new Set([...g.neighbors(input.focusedNodeId), input.focusedNodeId]);
  }
  const selectedClustersSet = new Set(input.selectedClusters);

  // Weight bounds: nodes for node sizing, edges (only when needed) for thickness.
  const nodeWeights: number[] = [];
  g.forEachNode((n) => nodeWeights.push((g.getNodeAttribute(n, 'weight') as number) ?? 1));
  const minNodeW = Math.min(...nodeWeights, 1);
  const maxNodeW = Math.max(...nodeWeights, 1);
  let minEdgeW = 0;
  let maxEdgeW = 1;
  if (cfg.computeEdgeSize) {
    const edgeWeights: number[] = [];
    g.forEachEdge((e) => edgeWeights.push((g.getEdgeAttribute(e, 'weight') as number) ?? 1));
    minEdgeW = Math.min(...edgeWeights, 0);
    maxEdgeW = Math.max(...edgeWeights, 1);
  }

  const highlightEdgeColor = cfg.highlightEdgeColor ?? '#94a3b8';
  const idleEdgeColor = cfg.idleEdgeColor ?? '#cbd5e1';

  g.forEachNode((n) => {
    const baseColor = cfg.getNodeColor(n);
    const baseSize = cfg.getNodeSize(n, minNodeW, maxNodeW);

    const isFocusedDimmed = isFocusActive && !focusNeighborsSet.has(n);
    const cluster = g.getNodeAttribute(n, 'cluster') as number | null | undefined;
    const isInClusterDimmed =
      isClusterActive &&
      (cluster === null || cluster === undefined || !selectedClustersSet.has(cluster));
    const isDimmed = isFocusedDimmed || isInClusterDimmed;

    if (isDimmed) {
      g.setNodeAttribute(n, 'color', `${baseColor}26`); // 15% alpha
      g.setNodeAttribute(n, 'size', baseSize * 0.6);
    } else {
      g.setNodeAttribute(n, 'color', baseColor);
      g.setNodeAttribute(n, 'size', baseSize);
    }
  });

  g.forEachEdge((edge, _attrs, source, target) => {
    if (cfg.computeEdgeSize) {
      g.setEdgeAttribute(edge, 'size', cfg.computeEdgeSize(edge, minEdgeW, maxEdgeW));
    }

    const isFocusedEdgeDimmed =
      isFocusActive && (!focusNeighborsSet.has(source) || !focusNeighborsSet.has(target));
    let isClusterEdgeDimmed = false;
    if (isClusterActive) {
      const sCluster = g.getNodeAttribute(source, 'cluster') as number | null;
      const tCluster = g.getNodeAttribute(target, 'cluster') as number | null;
      isClusterEdgeDimmed =
        sCluster === null ||
        tCluster === null ||
        !selectedClustersSet.has(sCluster) ||
        !selectedClustersSet.has(tCluster);
    }

    if (isFocusedEdgeDimmed || isClusterEdgeDimmed) {
      g.setEdgeAttribute(edge, 'color', '#f1f5f9');
    } else {
      g.setEdgeAttribute(
        edge,
        'color',
        isFocusActive || isClusterActive ? highlightEdgeColor : idleEdgeColor
      );
    }
  });
}
