import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import CitationPaperDetailPanel from '@/components/citation-paper-detail-panel.vue';
import type { CitationNode } from '@/types/biblio-citation';

/** Build a matched (in-library) CitationNode. */
function makeArticleNode(overrides: Partial<CitationNode> = {}): CitationNode {
  return {
    id: 'art-1',
    label: 'Smith et al. (2020)',
    title: 'Associations between trajectories of obesity prevalence',
    authors: 'Smith J, Doe A',
    year: 2020,
    journal: 'BMJ',
    numCited: 0,
    numReferences: 0,
    abstract: '',
    cluster: null,
    unmatched: false,
    ...overrides,
  };
}

const defaultProps = () => ({
  citingPapers: [] as { id: string; label: string }[],
  citedPapers: [] as { id: string; label: string }[],
  isolationMode: null,
});

describe('citation-paper-detail-panel.vue - hidden-references messaging', () => {
  it('shows the hidden-references hint when an article has numReferences but no in-graph edges', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        ...defaultProps(),
        paper: makeArticleNode({ numReferences: 51 }),
        citedPapers: [],
      },
    });

    const text = wrapper.text();
    // The "References" stat shows the count.
    expect(text).toContain('51');
    // The amber hint explains the references exist but aren't in the graph.
    expect(text).toContain('51 references not shown in graph');
    expect(text).toContain('not matched to included articles');
    // The "View in article" link is present.
    expect(wrapper.text().toLowerCase()).toContain('view in article');
  });

  it('shows the hidden-citations hint when an article has numCited but no in-graph edges', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        ...defaultProps(),
        paper: makeArticleNode({ numCited: 12 }),
        citingPapers: [],
      },
    });

    expect(wrapper.text()).toContain('12 citations not shown in graph');
  });

  it('combines both hidden references and citations in the summary', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        ...defaultProps(),
        paper: makeArticleNode({ numReferences: 51, numCited: 7 }),
        citingPapers: [],
        citedPapers: [],
      },
    });

    const text = wrapper.text();
    expect(text).toContain('51 references');
    expect(text).toContain('7 citations');
    // Both joined with " and ".
    expect(text).toMatch(/51 references.* and .*7 citations not shown in graph/);
  });

  it('omits the hidden-details hint when all references/citations are in the graph', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        ...defaultProps(),
        paper: makeArticleNode({ numReferences: 2, numCited: 1 }),
        citingPapers: [{ id: 'c1', label: 'X' }],
        citedPapers: [
          { id: 'r1', label: 'Y' },
          { id: 'r2', label: 'Z' },
        ],
      },
    });

    // No hint block because counts match edge-derived lists.
    expect(wrapper.text()).not.toContain('not shown in graph');
  });

  it('keeps the legacy "(no details available)" label for unmatched leaf nodes', () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        ...defaultProps(),
        paper: makeArticleNode({
          unmatched: true,
          numReferences: 5,
          referenceType: 'JOUR',
        }),
        citedPapers: [],
      },
    });

    const text = wrapper.text();
    // Unmatched leaves keep the legacy label.
    expect(text).toContain('(no details available)');
    // They do NOT get the amber hidden-details hint (no linked article to open).
    expect(text).not.toContain('not shown in graph');
    expect(text).not.toContain('View in article');
  });

  it('emits open-linked-record with the article id when the View-in-article link is clicked', async () => {
    const wrapper = mount(CitationPaperDetailPanel, {
      props: {
        ...defaultProps(),
        paper: makeArticleNode({ id: 'art-99', numReferences: 3 }),
        citedPapers: [],
      },
    });

    // Find the "View in article" button inside the hint block.
    const buttons = wrapper.findAll('button');
    const viewBtn = buttons.find((b) => b.text().toLowerCase().includes('view in article'));
    expect(viewBtn).toBeTruthy();
    await viewBtn!.trigger('click');

    const events = wrapper.emitted('openLinkedRecord') ?? wrapper.emitted('open-linked-record');
    expect(events).toBeTruthy();
    expect(events![0]).toEqual(['art-99']);
  });
});
