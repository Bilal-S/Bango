import { mount } from '@vue/test-utils';
import { describe, it, expect } from 'vitest';
import StatusBadge from '@/components/status-badge.vue';
import type { ArticleStatus } from '@/types';

describe('status-badge.vue', () => {
  it('renders "Duplicate" label and blue styles for duplicate status', () => {
    const wrapper = mount(StatusBadge, {
      props: { status: 'duplicate' },
    });
    expect(wrapper.text()).toBe('Duplicate');
    expect(wrapper.classes()).toContain('bg-blue-100');
    expect(wrapper.classes()).toContain('text-blue-800');
  });

  it('renders "Working" label and amber styles for working status', () => {
    const wrapper = mount(StatusBadge, {
      props: { status: 'working' },
    });
    expect(wrapper.text()).toBe('Working');
    expect(wrapper.classes()).toContain('bg-amber-100');
    expect(wrapper.classes()).toContain('text-amber-800');
  });

  it('renders "Included" label and emerald styles for included status', () => {
    const wrapper = mount(StatusBadge, {
      props: { status: 'included' },
    });
    expect(wrapper.text()).toBe('Included');
    expect(wrapper.classes()).toContain('bg-emerald-100');
    expect(wrapper.classes()).toContain('text-emerald-800');
  });

  it('renders "Rejected" label and rose styles for rejected status', () => {
    const wrapper = mount(StatusBadge, {
      props: { status: 'rejected' },
    });
    expect(wrapper.text()).toBe('Rejected');
    expect(wrapper.classes()).toContain('bg-rose-100');
    expect(wrapper.classes()).toContain('text-rose-800');
  });

  it('renders "Unknown" label and slate styles for other statuses', () => {
    const wrapper = mount(StatusBadge, {
      props: { status: 'unknown' as unknown as ArticleStatus },
    });
    expect(wrapper.text()).toBe('Unknown');
    expect(wrapper.classes()).toContain('bg-slate-100');
    expect(wrapper.classes()).toContain('text-slate-600');
  });
});
