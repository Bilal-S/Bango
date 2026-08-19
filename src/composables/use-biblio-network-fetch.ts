import { ref, computed } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import type Graph from 'graphology';

/**
 * Shared reactive state bundle for a bibliometric network composable.
 *
 * Create ONCE at module scope per composable (matching the historical
 * module-scoped `graph`/`loading`/`error` refs) so the graph survives
 * navigating away and back; the bundle is returned by every call of that
 * composable.
 */
export interface BiblioNetworkState {
  graph: Ref<Graph | null>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
  nodeCount: ComputedRef<number>;
  edgeCount: ComputedRef<number>;
}

/** Create the shared module-scoped state bundle for a network composable. */
export function createBiblioNetworkState(): BiblioNetworkState {
  const graph = ref<Graph | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const nodeCount = computed(() => graph.value?.order ?? 0);
  const edgeCount = computed(() => graph.value?.size ?? 0);
  return { graph, loading, error, nodeCount, edgeCount };
}

/** Normalize an unknown thrown value into a display message. */
export function networkErrorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * Run a network fetch with the shared loading/error/graph-reset scaffold.
 *
 * `fetcher` performs the IPC call and returns the built Graph, or null when
 * the backend reported no nodes (empty result). On success the graph is
 * stored; on failure the error message is captured, the graph is cleared,
 * and `onCatch` runs extra domain cleanup (e.g. clearing diagnostic meta).
 */
export async function runNetworkFetch(
  state: Pick<BiblioNetworkState, 'graph' | 'loading' | 'error'>,
  fetcher: () => Promise<Graph | null>,
  onCatch?: () => void
): Promise<void> {
  state.loading.value = true;
  state.error.value = null;

  try {
    state.graph.value = await fetcher();
  } catch (e: unknown) {
    state.error.value = networkErrorMessage(e);
    state.graph.value = null;
    onCatch?.();
  } finally {
    state.loading.value = false;
  }
}

/**
 * Linearly map a value from [inMin, inMax] onto [outMin, outMax].
 * Returns the output midpoint when the input range is degenerate.
 */
export function scaleToRange(
  value: number,
  inMin: number,
  inMax: number,
  outMin: number,
  outMax: number
): number {
  if (inMax === inMin) return (outMin + outMax) / 2;
  return outMin + ((value - inMin) / (inMax - inMin)) * (outMax - outMin);
}
