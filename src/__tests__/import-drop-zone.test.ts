import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import ImportDropZone from '@/components/import-drop-zone.vue';

describe('import-drop-zone.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('zotero_button_emits_zotero_selected', async () => {
    const wrapper = mount(ImportDropZone);
    const button = wrapper.find('.drop-zone__secondary');
    expect(button.exists()).toBe(true);
    expect(button.text()).toBe('Import from Zotero');

    await button.trigger('click');
    expect(wrapper.emitted('zoteroSelected')).toHaveLength(1);
  });
});
