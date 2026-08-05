/* Platform detection for keyboard-shortcut modifier selection.
 * Uses `navigator.platform` rather than a Tauri OS plugin to keep the helper
 * dependency-free, synchronous, and trivially unit-testable. */

const MAC_PLATFORMS = new Set(['MacIntel', 'Macintosh', 'Mac68K', 'iPhone', 'iPad', 'iPod']);
const WINDOWS_PLATFORMS = new Set(['Win32', 'Win64', 'Windows', 'WinCE']);

/** True on Apple platforms (macOS/iOS) for Cmd-based shortcuts. Returns false
 *  when `navigator` is absent (SSR / test runtimes). */
export function isMacPlatform(): boolean {
  if (typeof navigator === 'undefined' || typeof navigator.platform !== 'string') return false;
  return MAC_PLATFORMS.has(navigator.platform);
}

/** True on Windows, routing share links to Microsoft Store. Returns false when
 *  `navigator` is absent. */
export function isWindowsPlatform(): boolean {
  if (typeof navigator === 'undefined' || typeof navigator.platform !== 'string') return false;
  return WINDOWS_PLATFORMS.has(navigator.platform);
}

/** Shortcut modifier label for the active platform (e.g. `"Back (Alt+Left)"`). */
export const SHORTCUT_MODIFIER = isMacPlatform() ? 'Cmd' : 'Alt';
