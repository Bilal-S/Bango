import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import ConfidenceBar from '@/components/confidence-bar.vue';

describe('confidence-bar.vue', () => {
  it('renders percentage label for non-null confidence', () => {
    const wrapper = mount(ConfidenceBar, { props: { confidence: 0.856 } });
    expect(wrapper.text()).toContain('86%');
  });

  it('renders dashes when confidence is null', () => {
    const wrapper = mount(ConfidenceBar, { props: { confidence: null } });
    expect(wrapper.text()).toContain('---');
  });

  it('renders 10 segment dots', () => {
    const wrapper = mount(ConfidenceBar, { props: { confidence: 0.5 } });
    const dots = wrapper.findAll('span.inline-block');
    expect(dots).toHaveLength(10);
  });

  it('rounds confidence to nearest percentage', () => {
    const wrapper = mount(ConfidenceBar, { props: { confidence: 0.333 } });
    expect(wrapper.text()).toContain('33%');
  });

  it('shows 0% for confidence of 0', () => {
    const wrapper = mount(ConfidenceBar, { props: { confidence: 0 } });
    expect(wrapper.text()).toContain('0%');
  });

  it('shows 100% for confidence of 1', () => {
    const wrapper = mount(ConfidenceBar, { props: { confidence: 1 } });
    expect(wrapper.text()).toContain('100%');
  });
});
