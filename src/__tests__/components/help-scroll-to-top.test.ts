import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import HelpScrollToTop from '@/components/help/help-scroll-to-top.vue';

describe('help-scroll-to-top.vue', () => {
  beforeEach(() => {
    // No Pinia/router needed; component is pure.
  });

  it('renders a button element', () => {
    const wrapper = mount(HelpScrollToTop);
    expect(wrapper.find('button.help-scroll-to-top').exists()).toBe(true);
  });

  it('renders the vertical_align_top Material Symbols icon', () => {
    const wrapper = mount(HelpScrollToTop);
    const icon = wrapper.find('.help-scroll-to-top__icon');
    expect(icon.exists()).toBe(true);
    expect(icon.classes()).toContain('material-symbols-outlined');
    expect(icon.text()).toBe('vertical_align_top');
  });

  it('uses the default "Scroll to top" label for title and aria-label', () => {
    const wrapper = mount(HelpScrollToTop);
    const btn = wrapper.find('button');
    expect(btn.attributes('title')).toBe('Scroll to top');
    expect(btn.attributes('aria-label')).toBe('Scroll to top');
  });

  it('respects a custom label prop for title and aria-label', () => {
    const wrapper = mount(HelpScrollToTop, {
      props: { label: 'Back to top' },
    });
    const btn = wrapper.find('button');
    expect(btn.attributes('title')).toBe('Back to top');
    expect(btn.attributes('aria-label')).toBe('Back to top');
  });

  it('emits click when the button is clicked', async () => {
    const wrapper = mount(HelpScrollToTop);
    await wrapper.find('button').trigger('click');
    expect(wrapper.emitted('click')).toBeTruthy();
    expect(wrapper.emitted('click')).toHaveLength(1);
  });

  it('is a type="button" button (no form submit)', () => {
    const wrapper = mount(HelpScrollToTop);
    expect(wrapper.find('button').attributes('type')).toBe('button');
  });
});
