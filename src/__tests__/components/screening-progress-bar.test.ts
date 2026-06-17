import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import ScreeningProgressBar from '@/components/screening-progress-bar.vue';

describe('screening-progress-bar.vue', () => {
  it('renders completed/total and percentage', () => {
    const wrapper = mount(ScreeningProgressBar, {
      props: { completed: 5, total: 10, percentage: 50 },
    });
    expect(wrapper.text()).toContain('5 / 10');
    expect(wrapper.text()).toContain('50%');
  });

  it('sets fill width from percentage', () => {
    const wrapper = mount(ScreeningProgressBar, {
      props: { completed: 7, total: 10, percentage: 70 },
    });
    const fill = wrapper.find('.progress-bar__fill');
    expect(fill.attributes('style')).toContain('width: 70%');
  });

  it('renders 0% state', () => {
    const wrapper = mount(ScreeningProgressBar, {
      props: { completed: 0, total: 5, percentage: 0 },
    });
    expect(wrapper.text()).toContain('0 / 5');
    expect(wrapper.text()).toContain('0%');
  });

  it('renders 100% state', () => {
    const wrapper = mount(ScreeningProgressBar, {
      props: { completed: 10, total: 10, percentage: 100 },
    });
    const fill = wrapper.find('.progress-bar__fill');
    expect(fill.attributes('style')).toContain('width: 100%');
  });
});
