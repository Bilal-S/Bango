/**
 * Platform detection helpers for keyboard-shortcut modifier selection.
 *
 * Uses `navigator.platform` rather than a Tauri OS plugin to keep the helper
 * dependency-free, synchronous, and trivially unit-testable. The Webview's
 * `navigator.platform` reflects the host OS on all target platforms
 * (macOS reports `MacIntel`, Windows reports `Win32`, Linux reports `Linux*x86*`).
 */

const MAC_PLATFORMS = new Set(['MacIntel', 'Macintosh', 'Mac68K', 'iPhone', 'iPad', 'iPod']);
const WINDOWS_PLATFORMS = new Set(['Win32', 'Win64', 'Windows', 'WinCE']);

/**
 * Returns true when running on an Apple platform (macOS / iOS / iPadOS), used
 * to pick `Cmd`-based shortcuts instead of the Windows/Linux `Alt` ones.
 *
 * Resilient to `navigator` absence (SSR / unit-test runtimes): returns false
 * instead of throwing.
 */
export function isMacPlatform(): boolean {
  if (typeof navigator === 'undefined' || typeof navigator.platform !== 'string') return false;
  return MAC_PLATFORMS.has(navigator.platform);
}

/**
 * Returns true when running on a Windows platform, used to route the
 * "Share Bango" link to the Microsoft Store URL (with UTM attribution)
 * instead of the GitHub project URL. Returns false for macOS, Linux, and
 * unknown platforms.
 *
 * Resilient to `navigator` absence (SSR / unit-test runtimes): returns false
 * instead of throwing.
 */
export function isWindowsPlatform(): boolean {
  if (typeof navigator === 'undefined' || typeof navigator.platform !== 'string') return false;
  return WINDOWS_PLATFORMS.has(navigator.platform);
}

/**
 * Human-readable shortcut modifier label for the active platform, used in
 * tooltip/title text (e.g. `"Back (Alt+Left)"` vs `"Back (Cmd+[)"`).
 */
export const SHORTCUT_MODIFIER = isMacPlatform() ? 'Cmd' : 'Alt';
