import { describe, it, expect } from 'vitest';
import { useNavHistory } from '@/composables/use-nav-history';

describe('useNavHistory', () => {
  describe('initial state', () => {
    it('starts empty with current null and both directions disabled', () => {
      const nav = useNavHistory<string>();
      expect(nav.current.value).toBeNull();
      expect(nav.canGoBack.value).toBe(false);
      expect(nav.canGoForward.value).toBe(false);
      expect(nav.history.value).toEqual([]);
    });
  });

  describe('navigate', () => {
    it('pushes the first entry and sets it as current', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      expect(nav.current.value).toBe('a');
      expect(nav.history.value).toEqual(['a']);
      expect(nav.canGoBack.value).toBe(false);
      expect(nav.canGoForward.value).toBe(false);
    });

    it('pushes subsequent entries and grows the stack', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.navigate('c');
      expect(nav.history.value).toEqual(['a', 'b', 'c']);
      expect(nav.current.value).toBe('c');
      expect(nav.canGoBack.value).toBe(true);
      expect(nav.canGoForward.value).toBe(false);
    });

    it('skips when the entry equals the current one (no duplicates)', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.navigate('b'); // same as current
      expect(nav.history.value).toEqual(['a', 'b']);
      expect(nav.current.value).toBe('b');
    });

    it('truncates forward history when navigating after going back', () => {
      // Browser parity: a -> b -> c, back to a, then navigate d => [a, d].
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.navigate('c');
      nav.goBack(); // -> b
      nav.goBack(); // -> a
      expect(nav.current.value).toBe('a');
      nav.navigate('d');
      expect(nav.history.value).toEqual(['a', 'd']);
      expect(nav.current.value).toBe('d');
      expect(nav.canGoForward.value).toBe(false);
    });

    it('treats distinct object references as distinct entries', () => {
      const nav = useNavHistory<{ id: number }>();
      const a = { id: 1 };
      const b = { id: 1 }; // same shape, different reference
      nav.navigate(a);
      nav.navigate(b);
      expect(nav.history.value).toEqual([a, b]);
      expect(nav.current.value).toBe(b);
    });

    it('dedupes the exact same object reference (Object.is)', () => {
      const nav = useNavHistory<{ id: number }>();
      const a = { id: 1 };
      nav.navigate(a);
      nav.navigate(a); // same reference
      expect(nav.history.value).toEqual([a]);
    });
  });

  describe('goBack / goForward', () => {
    it('goBack moves the cursor back without removing entries', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.navigate('c');
      nav.goBack();
      expect(nav.current.value).toBe('b');
      expect(nav.history.value).toEqual(['a', 'b', 'c']); // unchanged
      expect(nav.canGoBack.value).toBe(true);
      expect(nav.canGoForward.value).toBe(true);
    });

    it('goForward moves the cursor forward', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.goBack();
      nav.goForward();
      expect(nav.current.value).toBe('b');
      expect(nav.canGoForward.value).toBe(false);
    });

    it('goBack is a no-op at the start of the history', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.goBack();
      expect(nav.current.value).toBe('a');
      expect(nav.canGoBack.value).toBe(false);
    });

    it('goForward is a no-op at the end of the history', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.goForward(); // already at end
      expect(nav.current.value).toBe('b');
    });

    it('goBack is a no-op on an empty history', () => {
      const nav = useNavHistory<string>();
      nav.goBack();
      expect(nav.current.value).toBeNull();
      expect(nav.canGoBack.value).toBe(false);
    });

    it('goForward is a no-op on an empty history', () => {
      const nav = useNavHistory<string>();
      nav.goForward();
      expect(nav.current.value).toBeNull();
      expect(nav.canGoForward.value).toBe(false);
    });

    it('round-trip: back to start then forward to end restores state', () => {
      const nav = useNavHistory<number>();
      nav.navigate(1);
      nav.navigate(2);
      nav.navigate(3);
      nav.goBack();
      nav.goBack(); // at 1
      expect(nav.current.value).toBe(1);
      expect(nav.canGoBack.value).toBe(false);
      nav.goForward();
      nav.goForward(); // at 3
      expect(nav.current.value).toBe(3);
      expect(nav.canGoForward.value).toBe(false);
      expect(nav.history.value).toEqual([1, 2, 3]);
    });
  });

  describe('clear', () => {
    it('wipes the whole history and resets the cursor', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.navigate('b');
      nav.goBack();
      nav.clear();
      expect(nav.history.value).toEqual([]);
      expect(nav.current.value).toBeNull();
      expect(nav.canGoBack.value).toBe(false);
      expect(nav.canGoForward.value).toBe(false);
    });

    it('is safe to call on an empty history', () => {
      const nav = useNavHistory<string>();
      nav.clear();
      expect(nav.current.value).toBeNull();
    });

    it('allows re-navigating after clear', () => {
      const nav = useNavHistory<string>();
      nav.navigate('a');
      nav.clear();
      nav.navigate('b');
      expect(nav.history.value).toEqual(['b']);
      expect(nav.current.value).toBe('b');
    });
  });
});
