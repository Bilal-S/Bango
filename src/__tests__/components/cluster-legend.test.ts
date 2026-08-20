import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import ClusterLegend from '@/components/cluster-legend.vue';

type Props = InstanceType<typeof ClusterLegend>['$props'];

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    clusterCount: 3,
    selectedClusters: [],
    ...overrides,
  } as Props;
}

function analyzeButton(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll('button').find((b) => b.text().includes('Analyze'));
}

describe('cluster-legend.vue', () => {
  it('renders_analyze_button_only_when_single_cluster_and_llm_ready', () => {
    // No selection: no trigger, pills still rendered.
    const none = mount(ClusterLegend, { props: baseProps() });
    expect(analyzeButton(none)).toBeUndefined();
    expect(none.findAll('button').length).toBe(4); // 3 pills + clear

    // Two clusters selected: still hidden.
    const two = mount(ClusterLegend, {
      props: baseProps({ selectedClusters: [0, 1], llmReady: true }),
    });
    expect(analyzeButton(two)).toBeUndefined();

    // Exactly one selected + LLM ready: visible in the heading row, before
    // the clear-filter icon (compact h-6 trigger labeled "Analyze").
    const one = mount(ClusterLegend, {
      props: baseProps({ selectedClusters: [1], llmReady: true }),
    });
    expect(analyzeButton(one)).toBeDefined();
    const buttons = one.findAll('button');
    expect(buttons[0]!.text()).toContain('Analyze');
    expect(buttons[0]!.attributes('title')).toBe("Ask the LLM what this cluster's members share");
    expect(buttons[0]!.attributes('class')).toContain('h-6');
    expect(buttons[1]!.attributes('title')).toBe('Clear cluster selection');
    // Label stays "Analyze" while in flight (glyph + disabled carry feedback).
    expect(buttons[0]!.text()).not.toContain('themes');

    // Exactly one selected but LLM not configured: hidden (canonical gate).
    const ungated = mount(ClusterLegend, {
      props: baseProps({ selectedClusters: [1], llmReady: false }),
    });
    expect(analyzeButton(ungated)).toBeUndefined();

    // In flight: rendered but disabled with a spinner glyph.
    const loading = mount(ClusterLegend, {
      props: baseProps({ selectedClusters: [1], llmReady: true, analysisLoading: true }),
    });
    const button = analyzeButton(loading);
    expect(button).toBeDefined();
    expect(button!.attributes('disabled')).toBeDefined();
    expect(button!.text()).toContain('progress_activity');

    // Zero clusters: nothing renders at all.
    const empty = mount(ClusterLegend, { props: baseProps({ clusterCount: 0 }) });
    expect(empty.find('div').exists()).toBe(false);
  });
});
