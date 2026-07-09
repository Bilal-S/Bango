import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useAuditStore, type ActivityFeedEntry } from '@/stores/audit';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const sampleFeed: ActivityFeedEntry[] = [
  {
    id: 'a1',
    timestamp: '2026-01-02T00:00:00Z',
    kind: 'audit',
    action: 'status_change',
    articleId: 'art1',
    details: 'changed',
    source: 'user',
    articleTitle: 'Paper',
    filename: null,
    count: null,
  },
  {
    id: 'i1',
    timestamp: '2026-01-01T00:00:00Z',
    kind: 'import',
    action: 'import',
    articleId: null,
    details: null,
    source: 'system',
    articleTitle: null,
    filename: 'papers.ris',
    count: 5,
  },
];

describe('useAuditStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty and uninitialized', () => {
    const store = useAuditStore();
    expect(store.feed).toEqual([]);
    expect(store.loading).toBe(false);
    expect(store.initialized).toBe(false);
    expect(store.offset).toBe(0);
    expect(store.hasMore).toBe(true);
  });

  it('fetch populates the feed', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_activity_feed') return Promise.resolve(sampleFeed);
      return Promise.resolve([]);
    });

    const store = useAuditStore();
    await store.fetch();

    expect(store.feed).toEqual(sampleFeed);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
    expect(store.offset).toBe(2);
    expect(store.hasMore).toBe(false);
  });

  it('fetchIfNeeded does nothing when already initialized', async () => {
    vi.mocked(tauriCommand).mockResolvedValue([]);
    const store = useAuditStore();
    store.initialized = true;
    await store.fetchIfNeeded();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('loadMore fetches next page and appends', async () => {
    const fullPage: ActivityFeedEntry[] = Array.from({ length: 10 }, (_, i) => ({
      id: `a${i}`,
      timestamp: '2026-01-01T00:00:00Z',
      kind: 'audit' as const,
      action: 'ai_screen',
      articleId: 'x',
      details: '',
      source: 'ai',
      articleTitle: null,
      filename: null,
      count: null,
    }));
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_activity_feed') return Promise.resolve(fullPage);
      return Promise.resolve([]);
    });

    const store = useAuditStore();
    await store.fetch();
    expect(store.feed.length).toBe(10);
    expect(store.hasMore).toBe(true);

    // Second page returns 3 items → hasMore becomes false
    const secondPage: ActivityFeedEntry[] = Array.from({ length: 3 }, (_, i) => ({
      id: `b${i}`,
      timestamp: '2025-01-01T00:00:00Z',
      kind: 'audit' as const,
      action: 'tag_add',
      articleId: 'y',
      details: '',
      source: 'user',
      articleTitle: null,
      filename: null,
      count: null,
    }));
    vi.mocked(tauriCommand).mockResolvedValue(secondPage);
    await store.loadMore();

    expect(store.feed.length).toBe(13);
    expect(store.hasMore).toBe(false);
    expect(store.offset).toBe(13);
  });

  it('loadMore is a no-op when no more pages', async () => {
    const store = useAuditStore();
    store.hasMore = false;
    await store.loadMore();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('loadMore guards against concurrent calls', async () => {
    const store = useAuditStore();
    store.hasMore = true;
    let resolveFirst: () => void;
    const p = new Promise<ActivityFeedEntry[]>((r) => {
      resolveFirst = () => r([]);
    });
    vi.mocked(tauriCommand).mockReturnValue(p);
    const first = store.loadMore();
    // Allow the first call to set loadingMore = true (microtask flush).
    await Promise.resolve();
    // Second call should be skipped because loadingMore is true
    await store.loadMore();
    expect(tauriCommand).toHaveBeenCalledTimes(1);
    resolveFirst!();
    await first;
  });

  it('invalidate resets all state', () => {
    const store = useAuditStore();
    store.feed = sampleFeed;
    store.initialized = true;
    store.hasMore = false;
    store.offset = 10;
    store.invalidate();
    expect(store.feed).toEqual([]);
    expect(store.initialized).toBe(false);
    expect(store.hasMore).toBe(true);
    expect(store.offset).toBe(0);
  });
});
