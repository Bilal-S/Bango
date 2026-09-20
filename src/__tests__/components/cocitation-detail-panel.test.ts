import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import CocitationDetailPanel from '@/components/cocitation-detail-panel.vue';
import type { CocitationNode } from '@/types/biblio-cocitation';

function makeNode(overrides: Partial<CocitationNode> = {}): CocitationNode {
  return {
    id: 'rp-1',
    label: '',
    title: 'Changes in soft-drink intake',
    authors: '',
    year: 2023,
    journal: 'BMJ',
    doi: null,
    citationCount: 10,
    coCitationCount: 4,
    matchedArticleId: null,
    matchedArticleStatus: null,
    abstract: '',
    referenceType: 'JOUR',
    ...overrides,
  };
}

function mountPanel(paper: CocitationNode | null) {
  return mount(CocitationDetailPanel, {
    props: { paper, coCitedPapers: [] },
  });
}

describe('cocitation-detail-panel.vue - authors rendering', () => {
  it('renders_json_authors_as_formatted_list_not_raw_array_syntax', () => {
    const wrapper = mountPanel(
      makeNode({ authors: JSON.stringify(['Pell, D', 'Mytton, O', 'Penney, TL']) })
    );
    const text = wrapper.text();
    expect(text).toContain('Pell, D, Mytton, O, Penney, TL');
    // The raw JSON payload must never leak into the panel.
    expect(text).not.toContain('[');
  });

  it('renders_plain_string_authors_unchanged', () => {
    const wrapper = mountPanel(makeNode({ authors: 'Smith J, Doe A' }));
    expect(wrapper.text()).toContain('Smith J, Doe A');
  });

  it('omits_the_authors_line_when_the_list_parses_empty', () => {
    const wrapper = mountPanel(makeNode({ authors: '[]' }));
    expect(wrapper.text()).toContain('Changes in soft-drink intake');
    expect(wrapper.text()).not.toContain('[]');
  });
});
