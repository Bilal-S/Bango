import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createRouter, createMemoryHistory } from 'vue-router';
import ScreeningStats from '@/components/screening-stats.vue';

function mountStats(props: {
  included: number;
  rejected: number;
  errors: number;
  estimatedTime: string;
}) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/articles', component: { template: '<div/>' } }],
  });
  return mount(ScreeningStats, {
    props,
    global: { plugins: [router] },
  });
}

describe('screening-stats.vue', () => {
  it('renders all four stat blocks', () => {
    const wrapper = mountStats({
      included: 5,
      rejected: 3,
      errors: 1,
      estimatedTime: '2m',
    });
    const items = wrapper.findAll('.stats__item');
    expect(items).toHaveLength(4);
  });

  it('renders numeric values', () => {
    const wrapper = mountStats({
      included: 7,
      rejected: 2,
      errors: 4,
      estimatedTime: '5m',
    });
    expect(wrapper.text()).toContain('7');
    expect(wrapper.text()).toContain('2');
    expect(wrapper.text()).toContain('4');
    expect(wrapper.text()).toContain('5m');
  });

  it('renders labels', () => {
    const wrapper = mountStats({
      included: 0,
      rejected: 0,
      errors: 0,
      estimatedTime: '-',
    });
    expect(wrapper.text()).toContain('Included');
    expect(wrapper.text()).toContain('Rejected');
    expect(wrapper.text()).toContain('Errors');
    expect(wrapper.text()).toContain('Est. Remaining');
  });

  it('renders clickable stat items with cursor styling', () => {
    const wrapper = mountStats({
      included: 1,
      rejected: 0,
      errors: 0,
      estimatedTime: '-',
    });
    const items = wrapper.findAll('.stats__item');
    // First three are clickable (included, rejected, errors); fourth is static.
    expect(items[0]!.classes()).toContain('stats__item--included');
    expect(items[1]!.classes()).toContain('stats__item--rejected');
    expect(items[2]!.classes()).toContain('stats__item--errors');
    expect(items[3]!.classes()).not.toContain('stats__item--included');
  });
});
