import { ref, nextTick } from 'vue';

/**
 * Global loading overlay singleton. Shows a fullscreen blurred overlay with a
 * spinner during long-running blocking operations (project import, demo load).
 *
 * `withOverlay` handles paint-yield: nextTick + requestAnimationFrame before
 * the blocking work, so the overlay renders before the main thread freezes.
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
