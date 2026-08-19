import { vi } from 'vitest';

/* Sigma needs WebGL, which happy-dom cannot provide. Stub the renderer
 * composable (docs/CLAUDE.md component-test rule) and expose the event
 * handler registry so tests can drive hover/click events like the real
 * renderer would.
 *
 * Import this module BEFORE importing the graph component under test so the
 * `vi.mock` registration below lands before the component resolves
 * `@/composables/use-sigma-renderer`. Each test-file module registry gets a
 * fresh `sigmaEvents` map; clear it in `beforeEach`.
 *
 * `vi.hoisted` guarantees the map exists before Vitest hoists the `vi.mock`
 * factory above this declaration - robust by construction instead of relying
 * on the factory running lazily. (Re-exported via a separate statement:
 * Vitest cannot `export const` a hoisted binding inline.) */
const sigmaEvents = vi.hoisted(() => new Map<string, (payload: unknown) => void>());
export { sigmaEvents };

vi.mock('@/composables/use-sigma-renderer', () => {
  interface FakeRenderer {
    on: (type: string, cb: (payload: unknown) => void) => void;
    refresh: () => void;
    kill: () => void;
  }
  const rendererRef: { value: FakeRenderer | null } = { value: null };
  return {
    useSigmaRenderer: () => ({
      renderer: rendererRef,
      initRenderer: () => {
        rendererRef.value = {
          on: (type, cb) => sigmaEvents.set(type, cb),
          refresh: () => {},
          kill: () => {},
        };
        return rendererRef.value;
      },
      destroyRenderer: () => {
        rendererRef.value = null;
      },
      locateNode: () => {},
      resetZoom: () => {},
      refresh: () => {},
    }),
  };
});
