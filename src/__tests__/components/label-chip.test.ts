import { mount } from '@vue/test-utils';
import { describe, it, expect } from 'vitest';
import LabelChip from '@/components/label-chip.vue';

describe('label-chip.vue', () => {
  it('renders label name and computes border/text colors with inner base dot', () => {
    const wrapper = mount(LabelChip, {
      props: {
        name: 'test-label',
        color: '#00ff00',
      },
    });

    expect(wrapper.text()).toBe('test-label');

    const containerSpan = wrapper.find('span');
    expect(containerSpan.exists()).toBe(true);
    const containerStyle = containerSpan.attributes('style');
    expect(containerStyle).toContain('color: #00ff00');
    expect(containerStyle).toContain('border-color: #8cff8c');

    const dotSpan = wrapper.find('span > span');
    expect(dotSpan.exists()).toBe(true);
    const dotStyle = dotSpan.attributes('style');
    expect(dotStyle).toContain('background-color: #00ff00');
  });

  it('handles empty/null color properties by generating a hash-based color', () => {
    const wrapper = mount(LabelChip, {
      props: {
        name: 'random-label-name',
        color: null,
      },
    });

    expect(wrapper.text()).toBe('random-label-name');
    const containerSpan = wrapper.find('span');
    const containerStyle = containerSpan.attributes('style');
    expect(containerStyle).toContain('color:');
    expect(containerStyle).toContain('border-color:');

    const dotSpan = wrapper.find('span > span');
    expect(dotSpan.exists()).toBe(true);
    const dotStyle = dotSpan.attributes('style');
    expect(dotStyle).toContain('background-color:');
  });
});
