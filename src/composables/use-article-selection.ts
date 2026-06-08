import { ref, computed, type Ref } from 'vue';
import type { Article } from '@/types';

export interface SelectionDeps {
  articles: Ref<Article[]>;
}

export function useArticleSelection(deps: SelectionDeps) {
  const { articles } = deps;

  const selectedIds = ref<Set<string>>(new Set());
  /** Tracks the anchor article for shift-click range selection. */
  const lastToggledId = ref<string | null>(null);

  const selectedCount = computed(() => selectedIds.value.size);

  const allSelected = computed(
    () => articles.value.length > 0 && selectedIds.value.size === articles.value.length
  );

  const someSelected = computed(() => selectedIds.value.size > 0 && !allSelected.value);

  function toggleSelect(id: string): void {
    const s = new Set(selectedIds.value);
    if (s.has(id)) {
      s.delete(id);
    } else {
      s.add(id);
    }
    selectedIds.value = s;
    lastToggledId.value = id;
  }

  /**
   * Toggle selection with optional shift-click range support.
   * When shiftKey is true, selects all articles between the last toggled
   * article and the clicked one (inclusive).
   */
  function toggleSelectRange(id: string, shiftKey: boolean): void {
    if (!shiftKey || lastToggledId.value === null) {
      toggleSelect(id);
      return;
    }

    const ids = articles.value.map((a) => a.id);
    const startIdx = ids.indexOf(lastToggledId.value);
    const endIdx = ids.indexOf(id);

    if (startIdx === -1 || endIdx === -1) {
      toggleSelect(id);
      return;
    }

    const lo = Math.min(startIdx, endIdx);
    const hi = Math.max(startIdx, endIdx);

    const s = new Set(selectedIds.value);
    for (let i = lo; i <= hi; i++) {
      s.add(ids[i]!);
    }
    selectedIds.value = s;
    // Keep the anchor so consecutive shift-clicks extend from the original anchor
  }

  function toggleSelectAll(): void {
    if (allSelected.value) {
      selectedIds.value = new Set();
      lastToggledId.value = null;
    } else {
      selectedIds.value = new Set(articles.value.map((a) => a.id));
      lastToggledId.value = null;
    }
  }

  function clearSelection(): void {
    selectedIds.value = new Set();
    lastToggledId.value = null;
  }

  return {
    selectedIds,
    selectedCount,
    allSelected,
    someSelected,
    toggleSelect,
    toggleSelectRange,
    toggleSelectAll,
    clearSelection,
  };
}
