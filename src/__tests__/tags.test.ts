import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useTagsStore } from '@/stores/tags';

// Mock: not running in Tauri → uses demo data
vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

describe('useTagsStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('starts with empty tags', () => {
    const store = useTagsStore();
    expect(store.tags).toEqual([]);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it('loads demo tags on fetchTags', async () => {
    const store = useTagsStore();
    await store.fetchTags();
    expect(store.tags.length).toBeGreaterThan(0);
    expect(store.initialized).toBe(true);
  });

  it('demo tags have required properties', async () => {
    const store = useTagsStore();
    await store.fetchTags();
    for (const tag of store.tags) {
      expect(tag).toHaveProperty('id');
      expect(tag).toHaveProperty('name');
      expect(tag).toHaveProperty('source');
      expect(tag).toHaveProperty('articleCount');
    }
  });

  it('fetchIfNeeded loads tags only once', async () => {
    const store = useTagsStore();
    await store.fetchIfNeeded();
    const count = store.tags.length;
    await store.fetchIfNeeded();
    expect(store.tags.length).toBe(count);
  });

  it('invalidate resets state', async () => {
    const store = useTagsStore();
    await store.fetchTags();
    expect(store.tags.length).toBeGreaterThan(0);
    store.invalidate();
    expect(store.tags).toEqual([]);
    expect(store.initialized).toBe(false);
  });

  it('createTag adds a tag in demo mode', async () => {
    const store = useTagsStore();
    await store.fetchTags();
    const before = store.tags.length;
    await store.createTag('new-tag');
    expect(store.tags.length).toBe(before + 1);
    expect(store.tags.some((t) => t.name === 'new-tag')).toBe(true);
  });

  it('createTag sets source to user_created', async () => {
    const store = useTagsStore();
    await store.fetchTags();
    await store.createTag('test-source-tag');
    const created = store.tags.find((t) => t.name === 'test-source-tag');
    expect(created).toBeDefined();
    expect(created!.source).toBe('user_created');
  });

  it('createTag sets articleCount to 0', async () => {
    const store = useTagsStore();
    await store.fetchTags();
    await store.createTag('count-tag');
    const created = store.tags.find((t) => t.name === 'count-tag');
    expect(created!.articleCount).toBe(0);
  });
});
