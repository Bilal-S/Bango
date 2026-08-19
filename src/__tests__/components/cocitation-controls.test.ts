import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import CocitationControls from '@/components/cocitation-controls.vue';

type Props = InstanceType<typeof CocitationControls>['$props'];

function baseProps(): Props {
  return {
    totalNodes: 20,
    totalEdges: 30,
    visibleNodes: 18,
    clusterCount: 2,
    scope: 'included',
    normalization: 'cosine',
    minCitationCount: 1,
    minCoCitation: 1,
    hideRejectedMatches: false,
    colorMode: 'cluster',
    layoutMode: 'fixed',
    paperLabels: [],
    minYear: 2018,
    maxYear: 2022,
    selectedClusters: [],
  } as Props;
}

describe('cocitation-controls.vue', () => {
  it('threshold_sliders_emit_updated_values', async () => {
    const wrapper = mount(CocitationControls, { props: baseProps() });

    /* Both range inputs emit numeric values on input:
     * [0] = Min. Citation Count, [1] = Min. Co-Citation. */
    const sliders = wrapper.findAll('input[type="range"]');
    expect(sliders.length).toBe(2);

    await sliders[0]!.setValue('5');
    expect(wrapper.emitted('min-citation-change')).toEqual([[5]]);

    await sliders[1]!.setValue('7');
    expect(wrapper.emitted('min-co-citation-change')).toEqual([[7]]);
  });
});
