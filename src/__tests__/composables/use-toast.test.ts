import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useToast } from '@/composables/use-toast';

describe('useToast', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    // Clear module-level toasts between tests by dismissing all known ids.
    const { toasts, dismiss } = useToast();
    for (const t of [...toasts.value]) {
      dismiss(t.id);
    }
    vi.useRealTimers();
  });

  it('starts with empty toasts', () => {
    const { toasts } = useToast();
    expect(toasts.value).toEqual([]);
  });

  it('show adds a toast with default type info and duration 3000', () => {
    vi.useFakeTimers();
    const { toasts, show } = useToast();
    show('Hello');
    expect(toasts.value).toHaveLength(1);
    expect(toasts.value[0]!.message).toBe('Hello');
    expect(toasts.value[0]!.type).toBe('info');
    expect(toasts.value[0]!.duration).toBe(3000);
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
});
