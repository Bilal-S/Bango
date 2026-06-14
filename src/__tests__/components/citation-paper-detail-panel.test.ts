import { mount } from '@vue/test-utils';
import { describe, it, expect } from 'vitest';
import CitationPaperDetailPanel from '@/components/citation-paper-detail-panel.vue';
import type { CitationNode } from '@/types/biblio-citation';

describe('citation-paper-detail-panel.vue', () => {
  it('renders select paper placeholder when paper is null', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: null,
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    expect(wrapper.text()).toContain('Select a node to view details.');
    expect(wrapper.find('.bg-emerald-100').exists()).toBe(false);
    expect(wrapper.find('.bg-slate-100').exists()).toBe(false);
  });

  it('renders "Reference Only" badge for unmatched papers', () => {
    const paper: CitationNode = {
      id: 'node-1',
      label: 'Smith et al. 2020',
      title: 'Title One',
      authors: 'A. Smith',
      year: 2020,
      journal: 'Journal of Testing',
      numCited: 0,
      numReferences: 0,
      abstract: 'An abstract example.',
      unmatched: true,
      cluster: null,
    };

    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper,
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    expect(wrapper.text()).toContain('Paper Details');
    expect(wrapper.text()).toContain('Reference Only');
    expect(wrapper.find('.bg-slate-100').exists()).toBe(true);
    expect(wrapper.find('.bg-emerald-100').exists()).toBe(false);
  });

  it('renders "Included" badge for matched/included papers', () => {
    const paper: CitationNode = {
      id: 'node-2',
      label: 'Jones et al. 2021',
      title: 'Title Two',
      authors: 'B. Jones',
      year: 2021,
      journal: 'Journal of Vue',
      numCited: 5,
      numReferences: 10,
      abstract: 'Vue is great.',
      unmatched: false,
      cluster: 1,
    };

    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper,
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    expect(wrapper.text()).toContain('Paper Details');
    expect(wrapper.text()).toContain('Included');
    expect(wrapper.find('.bg-emerald-100').exists()).toBe(true);
    expect(wrapper.find('.bg-slate-100').exists()).toBe(false);
  });
});
