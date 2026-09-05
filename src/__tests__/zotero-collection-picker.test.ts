import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => true,
  tauriCommand: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { tauriCommand } from '@/composables/use-tauri-command';
import ZoteroCollectionPicker from '@/components/zotero-collection-picker.vue';
import type { ImportPreview } from '@/composables/use-import';
import type { ZoteroCollectionPreview } from '@/types/zotero';

const collections = [
  { key: 'ROOT', name: 'Super Collection', parentKey: null },
  { key: 'CHILD', name: 'More Stuff', parentKey: 'ROOT' },
  { key: 'OTHER', name: 'Another Collection', parentKey: null },
];

const previewPayload: ZoteroCollectionPreview = {
  preview: {
    totalRecords: 1,
    validRecords: 1,
    errorCount: 0,
    duplicateCount: 0,
    errors: [],
    errorGroups: [],
    previewArticles: [
      { title: 'Alpha', authors: ['A'], publicationYear: 2020, journal: null, doi: null },
    ],
  } satisfies ImportPreview,
  articleKeys: ['ITEM1'],
  libraryVersion: 15,
  totalItems: 1,
  mappedArticles: 1,
  attachmentCount: 0,
  tagCount: 1,
};

function mountPicker() {
  return mount(ZoteroCollectionPicker);
}

describe('zotero-collection-picker.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders_collection_tree', async () => {
    vi.mocked(tauriCommand).mockResolvedValue(collections);
    const wrapper = mountPicker();
    await flushPromises();

    expect(tauriCommand).toHaveBeenCalledWith('get_zotero_collections');
    const items = wrapper.findAll('.zotero-picker__item');
    // Roots render in API order with children directly after their parents.
    expect(items.map((i) => i.text())).toEqual([
      'Super Collection',
      '- More Stuff',
      'Another Collection',
    ]);
    // The nested collection renders indented (deeper padding than roots).
    const paddings = items.map((i) => i.attributes('style'));
    expect(paddings[0]).toContain('12px');
    expect(paddings[1]).toContain('32px');
  });

  it('select_collection_fetches_preview', async () => {
    vi.mocked(tauriCommand).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'get_zotero_collections') return Promise.resolve(collections);
      if (cmd === 'get_zotero_collection_preview') return Promise.resolve(previewPayload);
      return Promise.reject(new Error(`unexpected: ${cmd} ${JSON.stringify(args)}`));
    });
    const wrapper = mountPicker();
    await flushPromises();

    await wrapper.findAll('.zotero-picker__item')[0]!.trigger('click');
    await flushPromises();

    expect(tauriCommand).toHaveBeenCalledWith('get_zotero_collection_preview', {
      collectionKey: 'ROOT',
    });
    const emitted = wrapper.emitted('collectionSelected');
    expect(emitted).toBeTruthy();
    expect(emitted![0]![0]).toMatchObject({
      collectionKey: 'ROOT',
      collectionName: 'Super Collection',
      libraryVersion: 15,
      articleKeys: ['ITEM1'],
    });
  });

  it('shows_error_state_on_fetch_failure', async () => {
    vi.mocked(tauriCommand).mockRejectedValue(new Error('Zotero is not running'));
    const wrapper = mountPicker();
    await flushPromises();

    const errorCard = wrapper.find('.zotero-picker__error');
    expect(errorCard.exists()).toBe(true);
    expect(errorCard.text()).toContain('Zotero is not running');
    // Retry re-runs the fetch.
    await errorCard.find('button').trigger('click');
    expect(tauriCommand).toHaveBeenCalledTimes(2);
  });
});
