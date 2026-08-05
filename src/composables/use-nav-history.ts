import { shallowRef, computed, readonly } from 'vue';

/**
 * Generic browser-like navigation history (back / forward).
 * Pure reactive logic - no DOM/Tauri deps.
 *
 * Uses `shallowRef` (not `ref`) because entries are immutable identifiers.
 * Semantics mirror a browser:
 * - `navigate(item)` pushes `item`, truncating forward history, skipping duplicates.
 * - `goBack()` / `goForward()` are no-ops at bounds.
 * - `clear()` wipes the stack.
 */
export function useNavHistory<T>() {
  const history = shallowRef<T[]>([]);
  const index = shallowRef(-1);

  const current = computed(() => (index.value >= 0 ? history.value[index.value]! : null));

  const canGoBack = computed(() => index.value > 0);

  const canGoForward = computed(() => index.value >= 0 && index.value < history.value.length - 1);

  /** Push entry, truncating forward history. Skips duplicates. */
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
