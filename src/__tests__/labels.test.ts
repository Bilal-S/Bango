import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useLabelsStore } from '@/stores/labels';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

describe('useLabelsStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('starts with empty labels', () => {
    const store = useLabelsStore();
    expect(store.labels).toEqual([]);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it('loads demo labels on fetchLabels', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    expect(store.labels.length).toBeGreaterThan(0);
    expect(store.initialized).toBe(true);
  });

  it('demo labels have required properties', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    for (const label of store.labels) {
      expect(label).toHaveProperty('id');
      expect(label).toHaveProperty('name');
      expect(label).toHaveProperty('source');
      expect(label).toHaveProperty('articleCount');
    }
  });

  it('fetchIfNeeded loads labels only once', async () => {
    const store = useLabelsStore();
    await store.fetchIfNeeded();
    const count = store.labels.length;
    await store.fetchIfNeeded();
    expect(store.labels.length).toBe(count);
  });

  it('invalidate resets state', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    expect(store.labels.length).toBeGreaterThan(0);
    store.invalidate();
    expect(store.labels).toEqual([]);
    expect(store.initialized).toBe(false);
  });

  it('createLabel adds a label in demo mode', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    const before = store.labels.length;
    await store.createLabel('new-label');
    expect(store.labels.length).toBe(before + 1);
    expect(store.labels.some((l) => l.name === 'new-label')).toBe(true);
  });

  it('createLabel sets source to user_created', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    await store.createLabel('test-source-label');
    const created = store.labels.find((l) => l.name === 'test-source-label');
    expect(created).toBeDefined();
    expect(created!.source).toBe('user_created');
  });

  it('createLabel sets articleCount to 0', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    await store.createLabel('count-label');
    const created = store.labels.find((l) => l.name === 'count-label');
    expect(created!.articleCount).toBe(0);
  });
});
