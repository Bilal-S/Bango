import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import KeywordControls from '@/components/keyword-controls.vue';

type Props = InstanceType<typeof KeywordControls>['$props'];

function baseProps(): Props {
  return {
    totalNodes: 20,
    totalEdges: 30,
    visibleNodes: 18,
    visibleEdges: 25,
    clusterCount: 2,
    keywordLabels: ['machine learning'],
    colorMode: 'cluster',
    minYear: 2018,
    maxYear: 2022,
    selectedClusters: [],
    sources: ['metadata', 'tags'],
    minOccurrences: 2,
    minCooccurrence: 2,
    layoutMode: 'fixed',
  } as Props;
}

describe('keyword-controls.vue', () => {
  it('threshold_sliders_emit_updated_values', async () => {
    const wrapper = mount(KeywordControls, { props: baseProps() });

    /* Both range sliders commit on `change`:
     * [0] = Min. Document Frequency, [1] = Min. Co-occurrence Strength. */
    const sliders = wrapper.findAll('input[type="range"]');
    expect(sliders.length).toBe(2);

    await sliders[0]!.setValue('5');
    await sliders[0]!.trigger('change');

    const events = wrapper.emitted('params-change');
    expect(events).toBeTruthy();
    const last = events![events!.length - 1]![0] as {
      sources: string[];
      minOccurrences: number;
      minCooccurrence: number;
    };
    expect(last.minOccurrences).toBe(5);
    expect(last.minCooccurrence).toBe(2);
    expect(last.sources).toEqual(['metadata', 'tags']);

    await sliders[1]!.setValue('4');
    await sliders[1]!.trigger('change');
    const after = wrapper.emitted('params-change') as unknown[][];
    const second = after[after.length - 1]![0] as { minCooccurrence: number };
    expect(second.minCooccurrence).toBe(4);
  });
});
