import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useLabelsStore } from '@/stores/labels';
import type { LabelWithCount } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockLabels: LabelWithCount[] = [
  { id: 'l1', name: 'priority-read', source: 'user_created', color: '#ef4444', articleCount: 12 },
  { id: 'l2', name: 'disputed', source: 'user_created', color: '#f59e0b', articleCount: 4 },
];

describe('useLabelsStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty', () => {
    const store = useLabelsStore();
    expect(store.labels).toEqual([]);
    expect(store.initialized).toBe(false);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it('fetchLabels populates labels from tauri', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.fetchLabels();

    expect(store.labels).toEqual(mockLabels);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
  });

  it('fetchIfNeeded skips when already initialized', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.fetchIfNeeded();
    expect(tauriCommand).toHaveBeenCalledTimes(1);

    // Second call is a no-op.
    await store.fetchIfNeeded();
    expect(tauriCommand).toHaveBeenCalledTimes(1);
  });

  it('fetchLabels handles errors gracefully', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('network error'));

    const store = useLabelsStore();
    await store.fetchLabels();

    expect(store.error).toBe('network error');
    expect(store.labels).toEqual([]);
    expect(store.loading).toBe(false);
  });

  it('createLabel invalidates and re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.fetchLabels();
    vi.mocked(tauriCommand).mockClear();

    await store.createLabel('new-label');
    expect(tauriCommand).toHaveBeenCalledWith('create_label', { request: { name: 'new-label' } });
    // After create, calls fetchLabels again.
    expect(tauriCommand).toHaveBeenCalledWith('get_labels_with_counts');
  });

  it('renameLabel calls rename command then re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.renameLabel('l1', 'updated-name');

    expect(tauriCommand).toHaveBeenCalledWith('rename_label', {
      request: { id: 'l1', newName: 'updated-name' },
    });
    expect(tauriCommand).toHaveBeenCalledWith('get_labels_with_counts');
  });

  it('deleteLabel removes label and re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.deleteLabel('l1');

    expect(tauriCommand).toHaveBeenCalledWith('delete_label', { id: 'l1' });
    expect(tauriCommand).toHaveBeenCalledWith('get_labels_with_counts');
  });

  it('updateLabelColor calls update command then re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.updateLabelColor('l1', '#ff0000');

    expect(tauriCommand).toHaveBeenCalledWith('update_label_color', {
      request: { id: 'l1', color: '#ff0000' },
    });
    expect(tauriCommand).toHaveBeenCalledWith('get_labels_with_counts');
  });

  it('suggestLabels calls suggest then re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.fetchLabels();
    vi.mocked(tauriCommand).mockClear();

    await store.suggestLabels();

    expect(tauriCommand).toHaveBeenCalledWith('suggest_labels');
    expect(tauriCommand).toHaveBeenCalledWith('get_labels_with_counts');
    expect(store.suggesting).toBe(false);
  });

  it('invalidate resets state', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockLabels);

    const store = useLabelsStore();
    await store.fetchLabels();
    expect(store.initialized).toBe(true);

    store.invalidate();
    expect(store.labels).toEqual([]);
    expect(store.initialized).toBe(false);
  });
});
