import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import NetworkSearchBox from '@/components/network-search-box.vue';
import type { NetworkSearchSuggestion } from '@/types/network-graph';

const suggestions: NetworkSearchSuggestion[] = [
  { key: 'smith', display: 'Smith, J.', detail: '4 papers', payload: 'Smith, J.' },
  { key: 'doe', display: 'Doe, A.', detail: '1 paper', payload: 'Doe, A.' },
];

function mountBox(overrides: Record<string, unknown> = {}) {
  return mount(NetworkSearchBox, {
    props: {
      modelValue: '',
      placeholder: 'Search authors…',
      suggestions: [],
      ...overrides,
    },
  });
}

describe('network-search-box.vue', () => {
  it('input_emits_update_model_value_and_input_event', async () => {
    const wrapper = mountBox();

    await wrapper.find('input[type="text"]').setValue('smith');

    expect(wrapper.emitted('update:modelValue')).toEqual([['smith']]);
    expect(wrapper.emitted('input')).toEqual([['smith']]);
    expect(wrapper.props('modelValue')).toBe('');
  });

  it('dropdown_lists_suggestions_with_detail_suffix', async () => {
    const wrapper = mountBox({ modelValue: 's', suggestions });

    await wrapper.find('input[type="text"]').trigger('focus');

    const items = wrapper.findAll('li');
    expect(items).toHaveLength(2);
    expect(items[0]!.text()).toContain('Smith, J.');
    expect(items[0]!.text()).toContain('(4 papers)');
    expect(items[1]!.text()).toContain('(1 paper)');
  });

  it('enter_selects_first_suggestion_and_copies_display', async () => {
    const wrapper = mountBox({ modelValue: 's', suggestions });

    await wrapper.find('input[type="text"]').trigger('focus');
    await wrapper.find('input[type="text"]').trigger('keydown.enter');

    expect(
      wrapper.emitted('update:modelValue')![wrapper.emitted('update:modelValue')!.length - 1]
    ).toEqual(['Smith, J.']);
    expect(wrapper.emitted('select-first')).toHaveLength(1);
    expect(wrapper.emitted('select-first')![0]![0]).toMatchObject({ payload: 'Smith, J.' });
  });

  it('clicking_suggestion_emits_select_with_payload', async () => {
    const wrapper = mountBox({ modelValue: 'd', suggestions });

    await wrapper.find('input[type="text"]').trigger('focus');
    await wrapper.findAll('li')[1]!.trigger('mousedown');

    expect(
      wrapper.emitted('update:modelValue')![wrapper.emitted('update:modelValue')!.length - 1]
    ).toEqual(['Doe, A.']);
    expect(wrapper.emitted('select')).toHaveLength(1);
    expect(wrapper.emitted('select')![0]![0]).toMatchObject({ payload: 'Doe, A.' });
  });

  it('clear_button_only_present_when_clearable_and_non_empty', async () => {
    const wrapper = mountBox({ clearable: true, modelValue: 'x' });
    expect(wrapper.find('button').exists()).toBe(true);

    await wrapper.find('button').trigger('click');
    expect(
      wrapper.emitted('update:modelValue')![wrapper.emitted('update:modelValue')!.length - 1]
    ).toEqual(['']);
    expect(wrapper.emitted('clear')).toHaveLength(1);

    const plain = mountBox({ modelValue: 'x' });
    expect(plain.find('button').exists()).toBe(false);
  });

  it('escape_emits_and_hides_dropdown', async () => {
    const wrapper = mountBox({ modelValue: 's', suggestions });

    await wrapper.find('input[type="text"]').trigger('focus');
    expect(wrapper.findAll('li')).toHaveLength(2);

    await wrapper.find('input[type="text"]').trigger('keydown.escape');
    expect(wrapper.emitted('escape')).toHaveLength(1);
    expect(wrapper.findAll('li')).toHaveLength(0);
  });
});
