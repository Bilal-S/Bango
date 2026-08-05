/**
 * Composable for computing the main path (SPC) in a Web Worker.
 * The worker is lazily instantiated and terminated on unmount.
 */

import { ref, onUnmounted, shallowRef, type Ref } from 'vue';
import type Graph from 'graphology';
import type { MainPathRequest, MainPathResponse } from '../workers/main-path-worker';

export function useMainPathWorker(graph: Ref<Graph | null>) {
  const mainPathNodes = ref<Set<string>>(new Set());
  const mainPathEdges = ref<Set<string>>(new Set());
  const computing = ref(false);

  // Worker instance held in a shallowRef so Vue doesn't try to make it reactive.
  const worker = shallowRef<Worker | null>(null);

  // Guard against worker callbacks firing after unmount during route transitions.
  let isUnmounted = false;

  function getWorker(): Worker | null {
    if (worker.value) return worker.value;
    try {
      // Vite handles `new Worker(new URL(...), { type: 'module' })` natively.
      worker.value = new Worker(new URL('../workers/main-path-worker.ts', import.meta.url), {
        type: 'module',
      });
      worker.value.onmessage = (event: MessageEvent<MainPathResponse>) => {
        if (isUnmounted) return; // component is gone - drop stale results
        mainPathNodes.value = new Set(event.data.nodes);
        mainPathEdges.value = new Set(event.data.edges);
        computing.value = false;
      };
      worker.value.onerror = (e: ErrorEvent) => {
        console.error('[main-path-worker]', e.message);
        if (isUnmounted) return;
        computing.value = false;
        mainPathNodes.value = new Set();
        mainPathEdges.value = new Set();
      };
      return worker.value;
    } catch {
      console.error('[main-path-worker] failed to instantiate worker');
      return null;
    }
  }

  /**
   * Serialize the graph and post to the worker.
   * No-op if the graph is null or empty.
   */
  function compute(): void {
    const g = graph.value;
    if (!g || g.order === 0) {
      mainPathNodes.value = new Set();
      mainPathEdges.value = new Set();
      return;
    }

    const w = getWorker();
    if (!w) return;

    const nodes: MainPathRequest['nodes'] = [];
    g.forEachNode((id, attrs) => {
      if (attrs.hidden !== true) {
        nodes.push({ id, year: attrs.year ?? null });
      }
    });

    const edges: MainPathRequest['edges'] = [];
    g.forEachEdge((id, _attrs, source, target) => {
      if (
        _attrs.hidden !== true &&
        g.getNodeAttribute(source, 'hidden') !== true &&
        g.getNodeAttribute(target, 'hidden') !== true
      ) {
        edges.push({ id, source, target });
      }
    });

    computing.value = true;
    w.postMessage({ nodes, edges });
  }

  /** Clear the main path state (does not terminate the worker). */
  function clear(): void {
    mainPathNodes.value = new Set();
    mainPathEdges.value = new Set();
  }

  onUnmounted(() => {
    isUnmounted = true;
    worker.value?.terminate();
    worker.value = null;
  });

  return {
    mainPathNodes,
    mainPathEdges,
    computing,
    compute,
    clear,
  };
}
