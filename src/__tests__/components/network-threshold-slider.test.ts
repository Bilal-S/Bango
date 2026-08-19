import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import NetworkThresholdSlider from '@/components/network-threshold-slider.vue';

function mountSlider(overrides: Record<string, unknown> = {}) {
  return mount(NetworkThresholdSlider, {
    props: {
      modelValue: 2,
      label: 'Min. Citations',
      min: 0,
      max: 10,
      ...overrides,
    },
  });
}

describe('network-threshold-slider.vue', () => {
  it('renders_label_and_current_value_badge', () => {
    const wrapper = mountSlider();
    expect(wrapper.text()).toContain('Min. Citations');
    expect(wrapper.find('.tabular-nums').text()).toBe('2');
  });

  it('input_emits_numeric_update_and_input', async () => {
    const wrapper = mountSlider();

    await wrapper.find('input[type="range"]').setValue('5');

    expect(wrapper.emitted('update:modelValue')).toEqual([[5]]);
    expect(wrapper.emitted('input')).toEqual([[5]]);
  });

  it('change_event_emits_commit', async () => {
    const wrapper = mountSlider();

    await wrapper.find('input[type="range"]').trigger('change');

    expect(wrapper.emitted('commit')).toHaveLength(1);
  });
});
