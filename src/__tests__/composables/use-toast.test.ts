import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useToast } from '@/composables/use-toast';

describe('useToast', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Clear module-level toasts and history between tests.
    const { toasts, dismiss, history, clearHistory } = useToast();
    for (const t of [...toasts.value]) {
      dismiss(t.id);
    }
    // Reset history if present (added after initial release).
    void history;
    clearHistory();
    vi.useRealTimers();
  });

  it('starts with empty toasts', () => {
    const { toasts } = useToast();
    expect(toasts.value).toEqual([]);
  });

  it('show adds a toast with default type info and duration 6000', () => {
    vi.useFakeTimers();
    const { toasts, show } = useToast();
    show('Hello');
    expect(toasts.value).toHaveLength(1);
    expect(toasts.value[0]!.message).toBe('Hello');
    expect(toasts.value[0]!.type).toBe('info');
    expect(toasts.value[0]!.duration).toBe(6000);
  });

  it('show accepts custom type and duration', () => {
    vi.useFakeTimers();
    const { toasts, show } = useToast();
    show('Saved', 'success', 5000);
    expect(toasts.value[0]!.type).toBe('success');
    expect(toasts.value[0]!.duration).toBe(5000);
  });

  it('persistent toast (duration 0) is not auto-dismissed', () => {
    vi.useFakeTimers();
    const { toasts, show } = useToast();
    show('Stays', 'warning', 0);
    vi.advanceTimersByTime(10_000);
    expect(toasts.value).toHaveLength(1);
  });

  it('auto-dismisses after duration', () => {
    vi.useFakeTimers();
    const { toasts, show } = useToast();
    show('Temp', 'info', 1000);
    expect(toasts.value).toHaveLength(1);
    vi.advanceTimersByTime(1000);
    expect(toasts.value).toHaveLength(0);
  });

  it('dismiss removes a specific toast by id', () => {
    vi.useFakeTimers();
    const { toasts, show, dismiss } = useToast();
    show('One');
    show('Two');
    const firstId = toasts.value[0]!.id;
    dismiss(firstId);
    expect(toasts.value).toHaveLength(1);
    expect(toasts.value[0]!.message).toBe('Two');
  });

  it('dismiss with unknown id is a no-op', () => {
    vi.useFakeTimers();
    const { toasts, show, dismiss } = useToast();
    show('One');
    dismiss(99999);
    expect(toasts.value).toHaveLength(1);
  });

  it('ids are unique and incrementing', () => {
    vi.useFakeTimers();
    const { toasts, show } = useToast();
    show('A');
    show('B');
    show('C');
    const ids = toasts.value.map((t) => t.id);
    expect(new Set(ids).size).toBe(3);
    expect(ids[1]).toBeGreaterThan(ids[0]!);
    expect(ids[2]).toBeGreaterThan(ids[1]!);
  });

  // ── History tracking ──────────────────────────────────────────────

  it('history is empty at start', () => {
    const { history } = useToast();
    expect(history.value).toEqual([]);
  });

  it('show records an entry in history', () => {
    vi.useFakeTimers();
    const { show, history } = useToast();
    show('Saved', 'success');
    expect(history.value).toHaveLength(1);
    expect(history.value[0]!.message).toBe('Saved');
    expect(history.value[0]!.type).toBe('success');
    expect(history.value[0]!.timestamp).toBeTypeOf('number');
  });

  it('history records multiple entries in order', () => {
    vi.useFakeTimers();
    const { show, history } = useToast();
    show('First', 'info');
    show('Second', 'error');
    show('Third', 'success');
    expect(history.value.map((h) => h.message)).toEqual(['First', 'Second', 'Third']);
  });

  it('dismiss does not remove from history', () => {
    vi.useFakeTimers();
    const { show, dismiss, toasts, history } = useToast();
    show('One');
    const id = toasts.value[0]!.id;
    dismiss(id);
    expect(toasts.value).toHaveLength(0);
    expect(history.value).toHaveLength(1);
    expect(history.value[0]!.message).toBe('One');
  });

  it('clearHistory empties the history but not active toasts', () => {
    vi.useFakeTimers();
    const { show, clearHistory, toasts, history } = useToast();
    show('Persist', 'warning', 0);
    expect(history.value).toHaveLength(1);
    clearHistory();
    expect(history.value).toEqual([]);
    // Active (persistent) toast is unaffected.
    expect(toasts.value).toHaveLength(1);
  });

  it('history is capped at MAX_HISTORY (100) entries, trimming oldest', () => {
    vi.useFakeTimers();
    const { show, history } = useToast();
    for (let i = 0; i < 105; i++) {
      show(`msg-${i}`, 'info', 0);
    }
    expect(history.value).toHaveLength(100);
    // Oldest 5 trimmed; the first remaining entry is msg-5.
    expect(history.value[0]!.message).toBe('msg-5');
    expect(history.value[99]!.message).toBe('msg-104');
  });
});
