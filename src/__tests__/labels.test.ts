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

  // ── mergeLabel (demo branch) ────────────────────────────────────────
  it('mergeLabel (demo) removes from-label and folds its count into the survivor', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    const from = store.labels[0]!;
    const into = store.labels[1]!;
    const expectedIntoCount = from.articleCount + into.articleCount;

    const result = await store.mergeLabel(from.id, into.id);

    expect(result.fromName).toBe(from.name);
    expect(result.intoName).toBe(into.name);
    expect(result.reassignedCount).toBe(from.articleCount);
    expect(result.alreadyHadSurvivorCount).toBe(0);
    expect(store.labels.find((l) => l.id === from.id)).toBeUndefined();
    expect(store.labels.find((l) => l.id === into.id)?.articleCount).toBe(expectedIntoCount);
  });

  it('mergeLabel (demo) throws when into-id is unknown', async () => {
    const store = useLabelsStore();
    await store.fetchLabels();
    const from = store.labels[0]!;
    await expect(store.mergeLabel(from.id, 'does-not-exist')).rejects.toThrow('Label not found');
  });
});
