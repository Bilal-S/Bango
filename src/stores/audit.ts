import { defineStore } from 'pinia';
import { ref } from 'vue';
import { isTauri, tauriCommand } from '@/composables/use-tauri-command';

/** Shape returned by the Rust `get_activity_feed` command - a single merged,
 *  timestamp-ordered stream of audit entries and import groups. */
export interface ActivityFeedEntry {
  id: string;
  timestamp: string;
  kind: 'audit' | 'import';
  action: string | null;
  articleId: string | null;
  details: string | null;
  source: string | null;
  articleTitle: string | null;
  filename: string | null;
  count: number | null;
}

const PAGE_SIZE = 10;

export const useAuditStore = defineStore('audit', () => {
  const feed = ref<ActivityFeedEntry[]>([]);
  const loading = ref(false);
  const initialized = ref(false);

  const offset = ref(0);
  const hasMore = ref(true);
  const loadingMore = ref(false);

  async function fetchIfNeeded(): Promise<void> {
    if (initialized.value || !isTauri()) return;
    await fetch();
  }

  /** Initial fetch - replaces feed with the first page. */
  async function fetch(): Promise<void> {
    loading.value = true;
    try {
      const entries = await tauriCommand<ActivityFeedEntry[]>('get_activity_feed', {
        limit: PAGE_SIZE,
        offset: 0,
      });
      feed.value = entries;
      offset.value = entries.length;
      hasMore.value = entries.length === PAGE_SIZE;
      initialized.value = true;
    } finally {
      loading.value = false;
    }
  }

  /** Load the next page of the merged feed. */
  async function loadMore(): Promise<void> {
    if (loadingMore.value || !hasMore.value) return;
    loadingMore.value = true;
    try {
      const entries = await tauriCommand<ActivityFeedEntry[]>('get_activity_feed', {
        limit: PAGE_SIZE,
        offset: offset.value,
      });
      if (entries.length > 0) {
        feed.value = [...feed.value, ...entries];
        offset.value += entries.length;
      }
      hasMore.value = entries.length === PAGE_SIZE;
    } finally {
      loadingMore.value = false;
    }
  }

  function invalidate(): void {
    feed.value = [];
    initialized.value = false;
    offset.value = 0;
    hasMore.value = true;
  }

  return {
    feed,
    loading,
    loadingMore,
    initialized,
    hasMore,
    offset,
    fetchIfNeeded,
    fetch,
    loadMore,
    invalidate,
  };
});
