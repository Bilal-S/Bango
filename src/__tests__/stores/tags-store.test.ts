import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useTagsStore } from '@/stores/tags';
import type { TagWithCount } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

import { tauriCommand } from '@/composables/use-tauri-command';

const mockTags: TagWithCount[] = [
  {
    id: '1',
    name: 'machine-learning',
    source: 'user_created',
    color: '#3b82f6',
    articleCount: 142,
  },
  { id: '2', name: 'clinical-trial', source: 'user_created', color: '#10b981', articleCount: 89 },
];

describe('useTagsStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it('starts empty', () => {
    const store = useTagsStore();
    expect(store.tags).toEqual([]);
    expect(store.initialized).toBe(false);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it('fetchTags populates tags from tauri', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.fetchTags();

    expect(store.tags).toEqual(mockTags);
    expect(store.initialized).toBe(true);
    expect(store.loading).toBe(false);
  });

  it('fetchIfNeeded skips when already initialized', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.fetchIfNeeded();
    expect(tauriCommand).toHaveBeenCalledTimes(1);

    await store.fetchIfNeeded();
    expect(tauriCommand).toHaveBeenCalledTimes(1);
  });

  it('fetchTags handles errors gracefully', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('network error'));

    const store = useTagsStore();
    await store.fetchTags();

    expect(store.error).toBe('network error');
    expect(store.tags).toEqual([]);
    expect(store.loading).toBe(false);
  });

  it('createTag invalidates and re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.fetchTags();
    vi.mocked(tauriCommand).mockClear();

    await store.createTag('new-tag');
    expect(tauriCommand).toHaveBeenCalledWith('create_tag', { request: { name: 'new-tag' } });
    expect(tauriCommand).toHaveBeenCalledWith('get_tags_with_counts');
  });

  it('renameTag calls rename command then re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.renameTag('1', 'updated-name');

    expect(tauriCommand).toHaveBeenCalledWith('rename_tag', {
      request: { id: '1', newName: 'updated-name' },
    });
    expect(tauriCommand).toHaveBeenCalledWith('get_tags_with_counts');
  });

  it('deleteTag removes tag and re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.deleteTag('1');

    expect(tauriCommand).toHaveBeenCalledWith('delete_tag', { id: '1' });
    expect(tauriCommand).toHaveBeenCalledWith('get_tags_with_counts');
  });

  it('updateTagColor calls update command then re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.updateTagColor('1', '#ff0000');

    expect(tauriCommand).toHaveBeenCalledWith('update_tag_color', {
      request: { id: '1', color: '#ff0000' },
    });
    expect(tauriCommand).toHaveBeenCalledWith('get_tags_with_counts');
  });

  it('suggestTags calls suggest then re-fetches', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.fetchTags();
    vi.mocked(tauriCommand).mockClear();

    await store.suggestTags();

    expect(tauriCommand).toHaveBeenCalledWith('suggest_tags');
    expect(tauriCommand).toHaveBeenCalledWith('get_tags_with_counts');
    expect(store.suggesting).toBe(false);
  });

  it('invalidate resets state', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(mockTags);

    const store = useTagsStore();
    await store.fetchTags();
    expect(store.initialized).toBe(true);

    store.invalidate();
    expect(store.tags).toEqual([]);
    expect(store.initialized).toBe(false);
  });
});
