import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useAuditStore } from '@/stores/audit';
import type { AuditEntry } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const sampleAudit: AuditEntry[] = [
  {
    id: 'a1',
    articleId: 'art1',
    timestamp: '2026-01-02T00:00:00Z',
    action: 'status_change',
    fromStatus: 'working',
    toStatus: 'included',
    details: 'changed',
    source: 'user',
    articleTitle: 'Paper',
  },
];

const sampleImports = [
  { id: 'i1', timestamp: '2026-01-01T00:00:00Z', filename: 'papers.ris', count: 5 },
];

describe('useAuditStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty and uninitialized', () => {
    const store = useAuditStore();
    expect(store.recentAudit).toEqual([]);
    expect(store.importActivities).toEqual([]);
    expect(store.loading).toBe(false);
    expect(store.initialized).toBe(false);
    expect(store.totalLoaded).toBe(0);
    expect(store.hasMoreAudit).toBe(true);
    expect(store.hasMoreImports).toBe(true);
  });

  it('fetch populates both audit and imports', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_recent_audit_entries') return Promise.resolve(sampleAudit);
      if (cmd === 'get_import_activities') return Promise.resolve(sampleImports);
      return Promise.resolve([]);
    });

    const store = useAuditStore();
    await store.fetch();

    expect(store.recentAudit).toEqual(sampleAudit);
    expect(store.importActivities).toEqual(sampleImports);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
    expect(store.totalLoaded).toBe(2);
  });

  it('fetchIfNeeded does nothing when already initialized', async () => {
    vi.mocked(tauriCommand).mockResolvedValue([]);
    const store = useAuditStore();
    store.initialized = true;
    await store.fetchIfNeeded();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('loadMore fetches additional pages when more available', async () => {
    // Prime the store with a full page (10 items) so hasMoreAudit stays true
    const fullAuditPage: AuditEntry[] = Array.from({ length: 10 }, (_, i) => ({
      id: `a${i}`,
      articleId: 'x',
      timestamp: '2026-01-01T00:00:00Z',
      action: 'import' as const,
      fromStatus: null,
      toStatus: null,
      details: '',
      source: 'system' as const,
      articleTitle: null,
    }));
    vi.mocked(tauriCommand).mockImplementation((cmd: string) => {
      if (cmd === 'get_recent_audit_entries') return Promise.resolve(fullAuditPage);
      if (cmd === 'get_import_activities') return Promise.resolve(sampleImports);
      return Promise.resolve([]);
    });

    const store = useAuditStore();
    await store.fetch();

    // Second page returns 0 results -> hasMore becomes false
    vi.mocked(tauriCommand).mockResolvedValue([]);
    await store.loadMore();

    expect(store.hasMoreAudit).toBe(false);
    expect(store.recentAudit.length).toBe(10);
  });

  it('loadMore is a no-op when no more pages', async () => {
    const store = useAuditStore();
    store.hasMoreAudit = false;
    store.hasMoreImports = false;
    await store.loadMore();
    expect(tauriCommand).not.toHaveBeenCalled();
  });

  it('loadMore guards against concurrent calls', async () => {
    const store = useAuditStore();
    store.hasMoreAudit = true;
    store.hasMoreImports = false; // narrow to a single command call
    let resolveFirst: () => void;
    const p = new Promise<unknown[]>((r) => {
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
    store.recentAudit = sampleAudit;
    store.importActivities = sampleImports;
    store.initialized = true;
    store.hasMoreAudit = false;
    store.invalidate();
    expect(store.recentAudit).toEqual([]);
    expect(store.importActivities).toEqual([]);
    expect(store.initialized).toBe(false);
    expect(store.hasMoreAudit).toBe(true);
  });
});
