import { ref, nextTick } from 'vue';

/**
 * Global loading overlay singleton.
 *
 * Shows a fullscreen blurred overlay with a spinner + dynamic message during
 * long-running blocking operations (project import, demo load). The overlay is
 * rendered by `app-shell.vue` at z-index 9999.
 *
 * The `withOverlay` helper handles the critical paint-yield sequence:
 * 1. Set reactive state (Vue queues DOM mutation)
 * 2. `await nextTick()` - Vue flushes the DOM mutation (overlay added to DOM)
 * 3. `await requestAnimationFrame` - browser paints the frame (overlay visible)
 * 4. Run the blocking async work (main thread may freeze, but overlay is painted)
 *
 * Without steps 2+3, the overlay DOM is in the render queue but never painted
 * before the blocking IPC call freezes the main thread.
 */
const isVisible = ref(false);
const message = ref('Loading...');

export function useLoadingOverlay() {
  function show(msg: string = 'Loading...'): void {
    message.value = msg;
    isVisible.value = true;
  }

  function hide(): void {
    isVisible.value = false;
  }

  /**
   * Run an async function while showing the overlay.
   * Handles the nextTick + requestAnimationFrame paint-yield automatically.
   */
  async function withOverlay<T>(msg: string, fn: () => Promise<T>): Promise<T> {
    show(msg);
    // Step 1: Wait for Vue to flush the reactive DOM update (overlay in DOM).
    await nextTick();
    // Step 2: Wait for the browser to actually paint the frame (overlay visible).
    await new Promise<number>(requestAnimationFrame);
    try {
      return await fn();
    } finally {
      hide();
    }
  }

  return { isVisible, message, show, hide, withOverlay };
}
