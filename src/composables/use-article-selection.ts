import { ref, computed, type Ref } from 'vue';
import type { Article } from '@/types';

export interface SelectionDeps {
  articles: Ref<Article[]>;
}

export function useArticleSelection(deps: SelectionDeps) {
  const { articles } = deps;

  const selectedIds = ref<Set<string>>(new Set());

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
  }

  function toggleSelectAll(): void {
    if (allSelected.value) {
      selectedIds.value = new Set();
    } else {
      selectedIds.value = new Set(articles.value.map((a) => a.id));
    }
  }

  function clearSelection(): void {
    selectedIds.value = new Set();
  }

  return {
    selectedIds,
    selectedCount,
    allSelected,
    someSelected,
    toggleSelect,
    toggleSelectAll,
    clearSelection,
  };
}
