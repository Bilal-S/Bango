import { describe, it, expect } from 'vitest';
import { nextTick, ref } from 'vue';
import {
  applyFilterSort,
  DEFAULT_SORT_DIR,
  DEFAULT_SORT_MODE,
  useTagLabelFilter,
  type FilterableItem,
} from '@/composables/use-tag-label-filter';

function makeItem(overrides: Partial<FilterableItem> = {}): FilterableItem {
  return { id: 'a', name: 'apple', articleCount: 1, color: null, ...overrides };
}

const FIXTURE: FilterableItem[] = [
  makeItem({ id: 'a', name: 'apple', articleCount: 5 }),
  makeItem({ id: 'b', name: 'Banana', articleCount: 10 }),
  makeItem({ id: 'c', name: 'cherry', articleCount: 5 }),
  makeItem({ id: 'd', name: 'date', articleCount: 1 }),
];

describe('applyFilterSort (pure helper)', () => {
  describe('filtering', () => {
    it('returns a sorted copy when the query is empty (no filter applied)', () => {
      const out = applyFilterSort(FIXTURE, '', 'alpha', 'asc');
      expect(out.map((i) => i.id)).toEqual(['a', 'b', 'c', 'd']);
    });

    it('treats a whitespace-only query as "no filter"', () => {
      const out = applyFilterSort(FIXTURE, '   ', 'alpha', 'asc');
      expect(out).toHaveLength(4);
    });

    it('filters case-insensitively on the name substring', () => {
      const out = applyFilterSort(FIXTURE, 'AN', 'alpha', 'asc');
      expect(out.map((i) => i.name)).toEqual(['Banana']);
    });

    it('returns an empty array when nothing matches', () => {
      const out = applyFilterSort(FIXTURE, 'zzz', 'alpha', 'asc');
      expect(out).toEqual([]);
    });

    it('does not mutate the source array', () => {
      const source = [...FIXTURE];
      applyFilterSort(source, '', 'alpha', 'desc');
      expect(source.map((i) => i.id)).toEqual(['a', 'b', 'c', 'd']);
    });
  });

  describe('alpha sort', () => {
    it('ascending = A-Z, case-insensitive', () => {
      const out = applyFilterSort(FIXTURE, '', 'alpha', 'asc');
      expect(out.map((i) => i.name)).toEqual(['apple', 'Banana', 'cherry', 'date']);
    });

    it('descending = Z-A', () => {
      const out = applyFilterSort(FIXTURE, '', 'alpha', 'desc');
      expect(out.map((i) => i.name)).toEqual(['date', 'cherry', 'Banana', 'apple']);
    });
  });

  describe('frequency sort', () => {
    it('ascending = smallest count first', () => {
      const out = applyFilterSort(FIXTURE, '', 'frequency', 'asc');
      expect(out.map((i) => i.articleCount)).toEqual([1, 5, 5, 10]);
    });

    it('descending = largest count first', () => {
      const out = applyFilterSort(FIXTURE, '', 'frequency', 'desc');
      expect(out.map((i) => i.articleCount)).toEqual([10, 5, 5, 1]);
    });

    it('ties break alphabetically A-Z regardless of direction (deterministic)', () => {
      // apple(5) and cherry(5) tie. Both directions keep apple before cherry.
      const asc = applyFilterSort(FIXTURE, '', 'frequency', 'asc');
      const desc = applyFilterSort(FIXTURE, '', 'frequency', 'desc');
      const ascTie = asc.filter((i) => i.articleCount === 5).map((i) => i.name);
      const descTie = desc.filter((i) => i.articleCount === 5).map((i) => i.name);
      expect(ascTie).toEqual(['apple', 'cherry']);
      expect(descTie).toEqual(['apple', 'cherry']);
    });
  });

  describe('combined filter + sort', () => {
    it('filters then sorts the remaining items', () => {
      // Items with an "a": apple(5), Banana(10), date(1). Frequency asc -> [date, apple, Banana].
      const out = applyFilterSort(FIXTURE, 'a', 'frequency', 'asc');
      expect(out.map((i) => i.id)).toEqual(['d', 'a', 'b']);
    });
  });
});

describe('useTagLabelFilter (reactive composable)', () => {
  function setup(items: FilterableItem[] = FIXTURE) {
    return useTagLabelFilter(() => items);
  }

  describe('initial state', () => {
    it('defaults to alpha A-Z ascending (matches historical behavior)', () => {
      const f = setup();
      expect(f.sortMode.value).toBe(DEFAULT_SORT_MODE);
      expect(f.sortDir.value).toBe(DEFAULT_SORT_DIR);
    });

    it('starts with the filter input collapsed and empty', () => {
      const f = setup();
      expect(f.filterOpen.value).toBe(false);
      expect(f.query.value).toBe('');
    });

    it('displayItems reflects the default alpha-asc sort on mount', () => {
      const f = setup();
      expect(f.displayItems.value.map((i) => i.name)).toEqual([
        'apple',
        'Banana',
        'cherry',
        'date',
      ]);
    });
  });

  describe('counts', () => {
    it('totalCount tracks the source length', () => {
      const f = setup();
      expect(f.totalCount.value).toBe(4);
    });

    it('shownCount equals total when no filter is active', () => {
      const f = setup();
      expect(f.shownCount.value).toBe(4);
    });

    it('shownCount drops below total when filtering narrows the list', async () => {
      const f = setup();
      f.query.value = 'a';
      await nextTick();
      expect(f.shownCount.value).toBe(3);
      expect(f.totalCount.value).toBe(4); // unchanged
    });

    it('isFiltering is true only when the query has non-whitespace text', async () => {
      const f = setup();
      expect(f.isFiltering.value).toBe(false);
      f.query.value = '   ';
      await nextTick();
      expect(f.isFiltering.value).toBe(false);
      f.query.value = ' a ';
      await nextTick();
      expect(f.isFiltering.value).toBe(true);
    });
  });

  describe('toggleSort', () => {
    it('clicking the active sort flips its direction', () => {
      const f = setup();
      // Default: alpha asc. Toggle alpha -> alpha desc.
      f.toggleSort('alpha');
      expect(f.sortMode.value).toBe('alpha');
      expect(f.sortDir.value).toBe('desc');
      // Toggle alpha again -> back to asc.
      f.toggleSort('alpha');
      expect(f.sortDir.value).toBe('asc');
    });

    it('clicking the inactive sort switches active and resets direction to asc', () => {
      const f = setup();
      // Default alpha asc. Switch to frequency -> frequency asc.
      f.toggleSort('frequency');
      expect(f.sortMode.value).toBe('frequency');
      expect(f.sortDir.value).toBe('asc');
    });

    it('switching sorts always starts the new sort at asc (no carryover direction)', () => {
      const f = setup();
      f.toggleSort('alpha'); // alpha desc
      f.toggleSort('frequency'); // switch -> frequency asc (NOT desc)
      expect(f.sortMode.value).toBe('frequency');
      expect(f.sortDir.value).toBe('asc');
    });

    it('displayItems re-sorts reactively when the sort changes', async () => {
      const f = setup();
      f.toggleSort('frequency');
      await nextTick();
      // Frequency asc: date(1), apple(5), cherry(5), Banana(10).
      expect(f.displayItems.value.map((i) => i.id)).toEqual(['d', 'a', 'c', 'b']);
    });
  });

  describe('clearFilter', () => {
    it('resets the query to an empty string', async () => {
      const f = setup();
      f.query.value = 'apple';
      await nextTick();
      expect(f.shownCount.value).toBe(1);
      f.clearFilter();
      await nextTick();
      expect(f.query.value).toBe('');
      expect(f.shownCount.value).toBe(4);
    });
  });

  describe('toggleFilterOpen', () => {
    it('flips the filterOpen flag', () => {
      const f = setup();
      expect(f.filterOpen.value).toBe(false);
      f.toggleFilterOpen();
      expect(f.filterOpen.value).toBe(true);
      f.toggleFilterOpen();
      expect(f.filterOpen.value).toBe(false);
    });
  });

  describe('reactivity to source changes', () => {
    it('displayItems recomputes when the source array reference changes', async () => {
      // The getter pattern relies on the caller supplying a reactive source
      // (in the real component, `() => props.items` works because `props`
      // is reactive). Here we mirror that with a `ref` so Vue can track the
      // dependency. A plain `let` would NOT trigger recomputation because the
      // closure over a non-reactive binding is invisible to the reactivity
      // system.
      const items = ref<FilterableItem[]>([makeItem({ id: 'a', name: 'apple', articleCount: 1 })]);
      const f = useTagLabelFilter(() => items.value);
      expect(f.displayItems.value).toHaveLength(1);
      items.value = [...items.value, makeItem({ id: 'b', name: 'banana', articleCount: 2 })];
      await nextTick();
      expect(f.displayItems.value).toHaveLength(2);
    });
  });
});
