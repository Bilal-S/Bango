import { mount } from '@vue/test-utils';
import { describe, it, expect } from 'vitest';
import CitationPaperDetailPanel from '@/components/citation-paper-detail-panel.vue';
import type { CitationNode } from '@/types/biblio-citation';

const basePaper: CitationNode = {
  id: 'node-1',
  label: 'Smith et al. 2020',
  title: 'Title One',
  authors: 'A. Smith',
  year: 2020,
  journal: 'Journal of Testing',
  numCited: 3,
  numReferences: 2,
  abstract: 'An abstract example.',
  unmatched: false,
  cluster: 0,
};

function makePaper(overrides: Partial<CitationNode> = {}): CitationNode {
  return { ...basePaper, ...overrides };
}

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
    const paper = makePaper({ unmatched: true, numCited: 0, numReferences: 0 });

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
    const paper = makePaper({ unmatched: false });

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

  it('shows both isolation buttons when no isolation is active', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper(),
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    expect(wrapper.find('[data-testid="isolate-ancestry-btn"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="isolate-progeny-btn"]').exists()).toBe(true);
  });

  it('keeps both isolation buttons visible even when isolation is active on this paper', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper(),
        citingPapers: [],
        citedPapers: [],
        isolationMode: { nodeId: 'node-1', direction: 'ancestry' },
      },
    });

    // Both buttons should still be present (the old behavior hid them).
    expect(wrapper.find('[data-testid="isolate-ancestry-btn"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="isolate-progeny-btn"]').exists()).toBe(true);
    // The active-isolation indicator badge should be shown.
    expect(wrapper.text()).toContain('Ancestry isolated');
  });

  it('emits isolate when clicking the inactive button while the other is active', async () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper(),
        citingPapers: [],
        citedPapers: [],
        isolationMode: { nodeId: 'node-1', direction: 'ancestry' },
      },
    });

    await wrapper.find('[data-testid="isolate-progeny-btn"]').trigger('click');
    expect(wrapper.emitted('isolate')).toBeTruthy();
    expect(wrapper.emitted('isolate')![0]).toEqual(['progeny']);
  });

  it('emits clear-isolation when clicking the currently active button', async () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper(),
        citingPapers: [],
        citedPapers: [],
        isolationMode: { nodeId: 'node-1', direction: 'ancestry' },
      },
    });

    await wrapper.find('[data-testid="isolate-ancestry-btn"]').trigger('click');
    expect(wrapper.emitted('clear-isolation')).toBeTruthy();
  });

  it('emits isolate when clicking a button with no prior isolation', async () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper(),
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    await wrapper.find('[data-testid="isolate-ancestry-btn"]').trigger('click');
    expect(wrapper.emitted('isolate')).toBeTruthy();
    expect(wrapper.emitted('isolate')![0]).toEqual(['ancestry']);
  });

  // ── open-linked-record button ─────────────────────────────────────────

  it('renders open-linked-record button for included papers', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper({ unmatched: false }),
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    const btn = wrapper.find('[data-testid="open-linked-record-btn"]');
    expect(btn.exists()).toBe(true);
    expect(btn.attributes('title')).toBe('open linked record');
    expect(btn.find('.material-symbols-outlined').text()).toBe('open_in_new');
  });

  it('hides open-linked-record button for unmatched (reference-only) papers', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper({ unmatched: true }),
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    expect(wrapper.find('[data-testid="open-linked-record-btn"]').exists()).toBe(false);
  });

  it('hides open-linked-record button when no paper is selected', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: null,
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    expect(wrapper.find('[data-testid="open-linked-record-btn"]').exists()).toBe(false);
  });

  it('emits open-linked-record with paper id when clicked', async () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        paper: makePaper({ unmatched: false }),
        citingPapers: [],
        citedPapers: [],
        isolationMode: null,
      },
    });

    await wrapper.find('[data-testid="open-linked-record-btn"]').trigger('click');
    expect(wrapper.emitted('open-linked-record')).toBeTruthy();
    expect(wrapper.emitted('open-linked-record')![0]).toEqual(['node-1']);
  });
});
