import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import NetworkExportMenu from '@/components/network-export-menu.vue';

describe('network-export-menu.vue', () => {
  it('menu_opens_on_toggle_and_lists_formats', async () => {
    const wrapper = mount(NetworkExportMenu);
    expect(wrapper.find('ul').exists()).toBe(false);

    await wrapper.find('button').trigger('click');
    expect(wrapper.find('ul').exists()).toBe(true);
    expect(wrapper.text()).toContain('PNG Image');
    expect(wrapper.text()).toContain('GEXF Network');
  });

  it('selecting_format_emits_and_closes_menu', async () => {
    const wrapper = mount(NetworkExportMenu);

    await wrapper.find('button').trigger('click');
    await wrapper.findAll('li')[1]!.trigger('click');

    expect(wrapper.emitted('select')).toEqual([['gexf']]);
    expect(wrapper.find('ul').exists()).toBe(false);
  });
});
