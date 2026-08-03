import { computed, ref } from 'vue';
import type { LabelWithCount, TagWithCount } from '@/types';

/**
 * The sortable item shape: anything with a `name` (for alphabetical + filter),
 * an `articleCount` (for frequency sort), and a `color` (passthrough for the
 * chip renderer). Both `TagWithCount` and `LabelWithCount` satisfy this
 * contract, so the composable is shared by both panels without duplicating
 * logic. `color` is included (not just `id`/`name`/`articleCount`) so the
 * composable's `displayItems` output carries everything the template needs
 * without the caller having to re-join the filtered list back to the source.
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

/**
 * Default sort on first render. Matches the historical behavior (the store
 * returned items in alphabetical order, ascending) so the initial view is
 * unchanged when the feature ships.
 */
export const DEFAULT_SORT_MODE: SortMode = 'alpha';
export const DEFAULT_SORT_DIR: SortDir = 'asc';

/**
 * Compare two items alphabetically, case-insensitive. Uses `localeCompare`
 * with `sensitivity: 'base'` so `A-Z` and `a-z` sort together (mirrors the
 * sort already used in `tags-section.vue` / `labels-section.vue`).
 */
function compareAlpha(a: FilterableItem, b: FilterableItem): number {
  return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
}

/**
 * Compare two items by frequency (article count). Higher count sorts first
 * when ascending (so "1-100 ascending" reads as "smallest first", matching
 * the `1-100` + `arrow_downward` cue in the UI). Ties break alphabetically
 * (A-Z) so the order is deterministic when two items share a count.
 */
function compareFrequency(a: FilterableItem, b: FilterableItem): number {
  if (a.articleCount !== b.articleCount) {
    return a.articleCount - b.articleCount;
  }
  return compareAlpha(a, b);
}

/**
 * Pure filter + sort over a list of filterable items. Extracted from the
 * reactive composable so it can be unit-tested in isolation (no Vue
 * reactivity required) and reused if a future caller needs a one-shot
 * derivation.
 *
 * Filter: case-insensitive substring match on `name`. Empty/whitespace query
 * passes all items through (still sorted).
 * Sort: by `mode`, with `direction` flipping the comparator. Frequency ties
 * always break A-Z regardless of direction, so toggling frequency direction
 * only swaps the count order, never the within-count alpha grouping.
 *
 * @param items   Source list (not mutated; a sorted copy is returned).
 * @param query   Filter text. Empty string = no filter.
 * @param mode    Sort field.
 * @param dir     Sort direction.
 * @returns       A new array, filtered + sorted.
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
    // Frequency ties already broke alphabetically A-Z; we do NOT invert that
    // tie-break under desc, only the primary count order. Alpha desc inverts
    // the whole comparator (there are no ties to preserve here).
    if (mode === 'frequency' && a.articleCount === b.articleCount) return base;
    return dir === 'asc' ? base : -base;
  });
  return filtered;
}

/**
 * Reactive filter + sort state for one Tags & Labels panel. Each panel mounts
 * its own instance - the two panels do not share state (per the feature spec).
 *
 * State is intentionally NOT persisted: it resets every time the panel
 * unmounts. Tags & Labels is a management surface, not a daily working list,
 * so carrying filter text across visits would surprise the user.
 *
 * The returned `displayItems` is a `computed` over the caller-supplied
 * `items` source, so the panel template can bind `v-for="item in
 * displayItems"` directly and stay in sync as the store updates.
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
