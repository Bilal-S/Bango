import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import GraphStatusOverlay from '@/components/graph-status-overlay.vue';

function baseProps(overrides: Record<string, unknown> = {}) {
  return {
    loading: false,
    isLayouting: false,
    error: null,
    empty: false,
    loadingLabel: 'Loading test network…',
    emptyIcon: 'hub',
    emptyText: 'Nothing here yet.',
    ...overrides,
  };
}

describe('graph-status-overlay.vue', () => {
  it('renders_loading_label_and_switches_to_layout_label', async () => {
    const wrapper = mount(GraphStatusOverlay, {
      props: baseProps({ loading: true }),
    });
    expect(wrapper.text()).toContain('Loading test network');
    expect(wrapper.text()).not.toContain('Computing layout');

    await wrapper.setProps({ loading: false, isLayouting: true });
    expect(wrapper.text()).toContain('Computing layout');
  });

  it('renders_error_with_retry_emit', async () => {
    const wrapper = mount(GraphStatusOverlay, {
      props: baseProps({ error: 'Fetch failed' }),
    });
    expect(wrapper.text()).toContain('Fetch failed');

    await wrapper.find('button').trigger('click');
    expect(wrapper.emitted('retry')).toHaveLength(1);
  });

  it('renders_simple_empty_state_text', () => {
    const wrapper = mount(GraphStatusOverlay, {
      props: baseProps({ empty: true }),
    });
    expect(wrapper.text()).toContain('Nothing here yet.');
    expect(wrapper.text()).not.toContain('Fetching failed');
  });

  it('renders_rich_empty_state_title_and_hint', () => {
    const wrapper = mount(GraphStatusOverlay, {
      props: baseProps({
        empty: true,
        emptyTitle: 'No co-citation data',
        emptyHint: 'Adjust thresholds.',
      }),
    });
    expect(wrapper.text()).toContain('No co-citation data');
    expect(wrapper.text()).toContain('Adjust thresholds.');
  });

  it('renders_nothing_when_loaded_without_error_or_empty', () => {
    const wrapper = mount(GraphStatusOverlay, { props: baseProps() });
    expect(wrapper.find('div').exists()).toBe(false);
  });
});
