/**
 * Create a debounced version of a function that delays execution until after
 * wait milliseconds have elapsed since the last invocation.
 *
 * @param fn - Function to debounce. Must not rely on a dynamic `this`
 * (module-scope functions and closures only).
 * @param delay - Milliseconds to wait after the last call.
 * @returns Debounced wrapper preserving the argument types of `fn`.
 */
export function debounce<T extends (...args: never[]) => void>(
  fn: T,
  delay: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout> | null = null;
  /* Single internal cast: `Parameters<T>` is not provably assignable to the
  constraint's `never[]` list, but the wrapper only forwards the args it got. */
  const invoke = fn as (...args: Parameters<T>) => void;
  return (...args: Parameters<T>) => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => invoke(...args), delay);
  };
}
