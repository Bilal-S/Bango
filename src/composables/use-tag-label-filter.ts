import { computed, ref } from 'vue';
import type { LabelWithCount, TagWithCount } from '@/types';

/**
 * Sortable item: `name`, `articleCount`, `color`. Both `TagWithCount` and
 * `LabelWithCount` satisfy this contract.
 */
export interface FilterableItem {
  id: string;
  name: string;
  articleCount: number;
  color: string | null;
}

/** Which field the list is sorted by. Exactly one is active at a time. */
export type SortMode = 'alpha' | 'frequency';

/** Sort direction. Ascending = A-Z for alpha, 1->N for frequency. */
export type SortDir = 'asc' | 'desc';

/** Default sort: alphabetical ascending (matches historical behavior). */
export const DEFAULT_SORT_MODE: SortMode = 'alpha';
export const DEFAULT_SORT_DIR: SortDir = 'asc';

/** Case-insensitive alphabetical comparison. */
function compareAlpha(a: FilterableItem, b: FilterableItem): number {
  return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
}

/** Compare by frequency. Higher count first when ascending. Ties break A-Z. */
function compareFrequency(a: FilterableItem, b: FilterableItem): number {
  if (a.articleCount !== b.articleCount) {
    return a.articleCount - b.articleCount;
  }
  return compareAlpha(a, b);
}

/**
 * Pure filter + sort. Filter: case-insensitive substring on `name`.
 * Sort: by `mode` with `direction`. Frequency ties always break A-Z.
 * Returns a new array.
 */
export function applyFilterSort<T extends FilterableItem>(
  items: readonly T[],
  query: string,
  mode: SortMode,
  dir: SortDir
): T[] {
  const q = query.trim().toLowerCase();
  const filtered = q ? items.filter((it) => it.name.toLowerCase().includes(q)) : [...items];
  const cmp = mode === 'alpha' ? compareAlpha : compareFrequency;
  filtered.sort((a, b) => {
    const base = cmp(a, b);
    /* Frequency ties already broke A-Z; we do NOT invert that tie-break under
    desc, only the primary count order. Alpha desc inverts the whole comparator. */
    if (mode === 'frequency' && a.articleCount === b.articleCount) return base;
    return dir === 'asc' ? base : -base;
  });
  return filtered;
}

/**
 * Reactive filter + sort state for one Tags & Labels panel.
 * State is NOT persisted - resets on every unmount.
 */
export function useTagLabelFilter<T extends FilterableItem>(items: () => readonly T[]) {
  const query = ref('');
  const sortMode = ref<SortMode>(DEFAULT_SORT_MODE);
  const sortDir = ref<SortDir>(DEFAULT_SORT_DIR);
  const filterOpen = ref(false);

  const displayItems = computed(() =>
    applyFilterSort(items(), query.value, sortMode.value, sortDir.value)
  );

  /** Total items in the source list (ignores the filter). */
  const totalCount = computed(() => items().length);

  /** Items visible after filtering + sorting. */
  const shownCount = computed(() => displayItems.value.length);

  /** True when a filter query is narrowing the list (query non-empty). */
  const isFiltering = computed(() => query.value.trim().length > 0);

  /**
   * Toggle the active sort. Rules (per spec):
   * - Clicking the currently-active sort flips its direction.
   * - Clicking the inactive sort makes it active and resets direction to the
   *   default ascending (A-Z for alpha, 1->N for frequency).
   */
  function toggleSort(mode: SortMode): void {
    if (sortMode.value === mode) {
      sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc';
    } else {
      sortMode.value = mode;
      sortDir.value = 'asc';
    }
  }

  /** Clear the filter text (the ClearableInput `x` handler). */
  function clearFilter(): void {
    query.value = '';
  }

  /**
   * Toggle the expanded/collapsed state of the filter input row. Clicking the
   * thin collapsed bar (or the caret) calls this.
   */
  function toggleFilterOpen(): void {
    filterOpen.value = !filterOpen.value;
  }

  return {
    query,
    sortMode,
    sortDir,
    filterOpen,
    displayItems,
    totalCount,
    shownCount,
    isFiltering,
    toggleSort,
    clearFilter,
    toggleFilterOpen,
  };
}

/** Convenience alias so callers can import a single typed name. */
export type TagLabelFilter<T extends FilterableItem> = ReturnType<typeof useTagLabelFilter<T>>;

/** Re-exported so the generic type resolves for TagWithCount callers. */
export type { TagWithCount, LabelWithCount };
