/**
 * Yield to the browser's rendering pipeline so that pending DOM updates
 * (e.g. showing a spinner) are painted before the caller proceeds with
 * expensive synchronous work such as a Tauri IPC call.
 *
 * Uses double requestAnimationFrame to guarantee the browser has
 * completed a paint cycle before resolving.
 */
export function nextPaint(): Promise<void> {
  return new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
  );
}
