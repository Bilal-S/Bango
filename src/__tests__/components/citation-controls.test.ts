import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import CitationControls from '@/components/citation-controls.vue';

type Props = InstanceType<typeof CitationControls>['$props'];

function baseProps(): Props {
  return {
    totalNodes: 20,
    totalEdges: 30,
    visibleNodes: 18,
    visibleEdges: 25,
    clusterCount: 2,
    paperLabels: [{ label: 'n1', display: 'Alpha Paper (2018)', searchText: 'alpha paper 2018' }],
    colorMode: 'cluster',
    layoutMode: 'fixed',
    minYear: 2018,
    maxYear: 2022,
    selectedClusters: [],
    showUnmatched: true,
    showMainPath: false,
    isolationMode: null,
  } as Props;
}

describe('citation-controls.vue', () => {
  it('threshold_sliders_emit_updated_values', async () => {
    const wrapper = mount(CitationControls, { props: baseProps() });

    /* First range input is "Min. Citations Received" (the other two range
     * inputs are the dual-handle Time-Slice year sliders). */
    const sliders = wrapper.findAll('input[type="range"]');
    expect(sliders.length).toBeGreaterThanOrEqual(1);
    await sliders[0]!.setValue('5');

    const events = wrapper.emitted('filter-change');
    expect(events).toBeTruthy();
    const last = events![events!.length - 1]![0] as {
      minCitations: number;
      showIsolated: boolean;
      search: string;
      yearRange: [number, number] | null;
    };
    expect(last.minCitations).toBe(5);
    expect(last.showIsolated).toBe(true);
    expect(last.search).toBe('');
    expect(last.yearRange).toBeNull();
  });

  it('search_input_emits_filter_string', async () => {
    const wrapper = mount(CitationControls, { props: baseProps() });

    const search = wrapper.find('input[type="text"]');
    await search.setValue('deep learning');

    const events = wrapper.emitted('filter-change');
    expect(events).toBeTruthy();
    const last = events![events!.length - 1]![0] as { search: string };
    expect(last.search).toBe('deep learning');
  });
});
