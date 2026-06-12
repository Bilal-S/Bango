import { mount } from '@vue/test-utils';
import { describe, it, expect } from 'vitest';
import TagChip from '@/components/tag-chip.vue';

describe('tag-chip.vue', () => {
  it('renders tag name and computes color scheme style bindings', () => {
    const wrapper = mount(TagChip, {
      props: {
        name: 'test-tag',
        color: '#ff0000',
      },
    });

    expect(wrapper.text()).toBe('test-tag');
    const span = wrapper.find('span');
    expect(span.exists()).toBe(true);

    const style = span.attributes('style');
    // We expect color values to be resolved as hex strings in the mock DOM environment
    expect(style).toContain('background-color: #ffd9d9');
    expect(style).toContain('color: #ff0000');
    expect(style).toContain('border-color: #ff8c8c');
  });

  it('handles empty/null color properties by generating a hash-based color', () => {
    const wrapper = mount(TagChip, {
      props: {
        name: 'random-tag-name',
        color: null,
      },
    });

    expect(wrapper.text()).toBe('random-tag-name');
    const span = wrapper.find('span');
    const style = span.attributes('style');
    expect(style).toContain('background-color:');
    expect(style).toContain('color:');
    expect(style).toContain('border-color:');
  });
});
