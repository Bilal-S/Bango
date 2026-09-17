import { describe, it, expect, vi, beforeEach } from 'vitest';
import { debounce } from '@/utils/debounce';

describe('debounce', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it('does not call the function immediately', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);
    debounced();
    expect(fn).not.toHaveBeenCalled();
  });

  it('calls the function after the delay', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);
    debounced();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('resets the timer on subsequent calls within the window', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);
    debounced();
    vi.advanceTimersByTime(50);
    debounced();
    vi.advanceTimersByTime(50);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(50);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('passes arguments to the wrapped function', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 50);
    debounced('a', 1, true);
    vi.advanceTimersByTime(50);
    expect(fn).toHaveBeenCalledWith('a', 1, true);
  });

  it('calls the function only once for rapid bursts', () => {
    const fn = vi.fn();
    const debounced = debounce(fn, 100);
    for (let i = 0; i < 5; i++) {
      debounced(i);
    }
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(fn).toHaveBeenCalledWith(4);
  });

  it('supports multiple independent debounced wrappers', () => {
    const fn1 = vi.fn();
    const fn2 = vi.fn();
    const d1 = debounce(fn1, 100);
    const d2 = debounce(fn2, 100);
    d1();
    d2();
    vi.advanceTimersByTime(100);
    expect(fn1).toHaveBeenCalledTimes(1);
    expect(fn2).toHaveBeenCalledTimes(1);
  });

  it('does not forward this to the wrapped function', () => {
    /* Contract: module-scope debouncing only. The wrapper never forwards a
     * dynamic `this` to the wrapped function (no call site relies on it). */
    const sentinel = { marker: 'sentinel' };
    const seen: { self: unknown; arg: string | null } = { self: 'unset', arg: null };
    const fn = function (this: unknown, v: string): void {
      seen.self = this;
      seen.arg = v;
    };
    const debounced = debounce(fn, 100);

    debounced.call(sentinel, 'x');
    vi.advanceTimersByTime(100);

    expect(seen.arg).toBe('x');
    expect(seen.self).not.toBe(sentinel);
  });

  it('supports async functions with typed arguments', () => {
    const calls: Array<{ query: string; n: number }> = [];
    const debounced = debounce(async (query: string, n: number): Promise<void> => {
      calls.push({ query, n });
    }, 50);

    debounced('wiki', 3);
    debounced('wiki-2', 4);
    vi.advanceTimersByTime(50);

    expect(calls).toEqual([{ query: 'wiki-2', n: 4 }]);
  });
});
