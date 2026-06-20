import { shallowRef, computed, readonly } from 'vue';

/**
 * Generic browser-like navigation history (back / forward).
 *
 * Extracted as pure reactive logic so it can be unit-tested without any DOM or
 * Tauri dependencies, following the project pattern (see `use-network-view.ts`,
 * `use-startup-upgrade.ts`). The owning view is responsible for routing user
 * actions (sidebar clicks, `[[wikilink]]` clicks, keyboard shortcuts) through
 * `navigate()` / `goBack()` / `goForward()` / `clear()`.
 *
 * `shallowRef` is used (not `ref`) because entries are immutable identifiers
 * (e.g. wiki page slugs); we only need reactivity on the array and the cursor,
 * not deep reactivity on the entries themselves. This also sidesteps Vue's
 * `UnwrapRefSimple<T>` generic-unwrapping constraints.
 *
 * Semantics mirror a browser's history stack:
 * - `navigate(item)` pushes `item`, truncating any forward history, and skips
 *   when `item` equals the current entry (no duplicate back-to-back entries).
 * - `goBack()` / `goForward()` move the cursor; they are no-ops at the bounds.
 * - `clear()` wipes the whole stack.
 *
 * @typeParam T - The kind of entry tracked (e.g. a wiki page slug `string`).
 */
export function useNavHistory<T>() {
  const history = shallowRef<T[]>([]);
  const index = shallowRef(-1);

  const current = computed(() => (index.value >= 0 ? history.value[index.value]! : null));

  const canGoBack = computed(() => index.value > 0);

  const canGoForward = computed(() => index.value >= 0 && index.value < history.value.length - 1);

  /**
   * Push a new entry. Truncates any forward history (browser parity) and skips
   * when the entry equals the current one to avoid duplicate back-to-back
   * entries (e.g. clicking the already-active sidebar item).
   */
  function navigate(item: T): void {
    const cur = current.value;
    if (cur !== null && Object.is(cur, item)) return;
    // Drop any forward entries (everything after the current index).
    const next = index.value >= 0 ? history.value.slice(0, index.value + 1) : [];
    next.push(item);
    history.value = next;
    index.value = next.length - 1;
  }

  /** Move the cursor back; no-op at the start of the history. */
  function goBack(): void {
    if (canGoBack.value) {
      index.value -= 1;
    }
  }

  /** Move the cursor forward; no-op at the end of the history. */
  function goForward(): void {
    if (canGoForward.value) {
      index.value += 1;
    }
  }

  /** Wipe the whole history and reset the cursor. */
  function clear(): void {
    history.value = [];
    index.value = -1;
  }

  return {
    current,
    canGoBack,
    canGoForward,
    history: readonly(history),
    navigate,
    goBack,
    goForward,
    clear,
  };
}
