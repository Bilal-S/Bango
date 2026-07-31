import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import CitationResultCard from '@/components/citation-result-card.vue';
import type { CitationMatch } from '@/types/citation-finder';

function makeMatch(overrides: Partial<CitationMatch> = {}): CitationMatch {
  return {
    articleId: 'art-1',
    title: 'Sugar levy effects on childhood obesity',
    authors: ['Smith, J.'],
    publicationYear: 2024,
    journal: 'BMJ Global Health',
    doi: '10.1136/bmjgh-2024-009999',
    matchedPassage: 'The sugar tax reduced obesity significantly.',
    sectionOrigin: 'Results',
    classification: 'validating',
    relevanceExplanation: 'Directly supports the claim.',
    misrepresentsSource: false,
    confidence: 0.92,
    highlightedSentences: [],
    ...overrides,
  };
}

function mountCard(
  props: Partial<{ match: CitationMatch; style: string; ieeeIndex: number }> = {}
) {
  return mount(CitationResultCard, {
    props: {
      match: makeMatch(),
      style: 'APA',
      ieeeIndex: 1,
      ...props,
    } as InstanceType<typeof CitationResultCard>['$props'],
  });
}

describe('CitationResultCard', () => {
  it('renders_metadata_passage_badge_confidence', () => {
    const wrapper = mountCard();
    const text = wrapper.text();
    expect(text).toContain('Smith, J.');
    expect(text).toContain('(2024)');
    expect(text).toContain('BMJ Global Health');
    expect(text).toContain('The sugar tax reduced obesity significantly.');
    expect(text).toContain('§Results');
    expect(text).toContain('✓ Validating');
    // confidence 0.92 → "92% match"
    expect(text).toContain('92% match');
    expect(text).toContain('Directly supports the claim.');
  });

  it('renders the opposing badge for an opposing classification', () => {
    const wrapper = mountCard({
      match: makeMatch({ classification: 'opposing', confidence: 0.3 }),
    });
    expect(wrapper.text()).toContain('✗ Opposing');
    expect(wrapper.text()).toContain('30% match');
  });

  it('sectionOrigin_null_omits_badge', () => {
    const wrapper = mountCard({
      match: makeMatch({ sectionOrigin: null }),
    });
    // The §Section badge should NOT render when sectionOrigin is null.
    expect(wrapper.text()).not.toContain('§');
    expect(wrapper.find('.citation-card__section-badge').exists()).toBe(false);
  });

  it('emits copy with the formatted citation text', async () => {
    const wrapper = mountCard({ style: 'APA', ieeeIndex: 1 });
    const copyBtn = wrapper.find('.citation-card__btn--copy');
    expect(copyBtn.exists()).toBe(true);
    await copyBtn.trigger('click');
    const copyEvents = wrapper.emitted('copy');
    expect(copyEvents).toBeDefined();
    expect(copyEvents?.[0]?.[0]).toMatch(/^\(Smith, 2024\)/);
  });

  it('emits view with the article id', async () => {
    const wrapper = mountCard();
    const viewBtn = wrapper.find('.citation-card__btn--view');
    expect(viewBtn.exists()).toBe(true);
    await viewBtn.trigger('click');
    const viewEvents = wrapper.emitted('view');
    expect(viewEvents).toBeDefined();
    expect(viewEvents?.[0]?.[0]).toBe('art-1');
  });

  it('truncates a long DOI for display', () => {
    const longDoi = '10.1136/bmjgh-2024-009999-very-long-suffix-abcdef123456';
    const wrapper = mountCard({
      match: makeMatch({ doi: longDoi }),
    });
    // The display DOI keeps the prefix + last 12 chars; it should NOT be the
    // full string.
    expect(wrapper.text()).not.toContain(longDoi);
    expect(wrapper.text()).toContain('doi:');
  });

  it('hides the DOI segment when doi is null', () => {
    const wrapper = mountCard({
      match: makeMatch({ doi: null }),
    });
    expect(wrapper.text()).not.toContain('doi:');
  });

  // ── Progressive passage disclosure ─────────────────────────────────────

  it('renders full passage and no toggle when highlightedSentences is empty', () => {
    // The legacy fallback: no highlights → full passage in quotes, no
    // "Show full passage" toggle.
    const wrapper = mountCard({
      match: makeMatch({
        matchedPassage: 'The full passage with no highlights.',
        highlightedSentences: [],
      }),
    });
    expect(wrapper.text()).toContain('The full passage with no highlights.');
    expect(wrapper.find('.citation-card__expand-toggle').exists()).toBe(false);
  });

  it('collapses to highlighted snippets by default when highlightedSentences non-empty', () => {
    // Default (collapsed): only the highlighted snippets render, NOT the
    // surrounding full-passage text.
    const wrapper = mountCard({
      match: makeMatch({
        matchedPassage: 'Context sentence. KEY SENTENCE one. Filler. KEY SENTENCE two. End.',
        highlightedSentences: ['KEY SENTENCE one', 'KEY SENTENCE two'],
      }),
    });
    // The two snippets render.
    expect(wrapper.text()).toContain('KEY SENTENCE one');
    expect(wrapper.text()).toContain('KEY SENTENCE two');
    // The non-highlighted context does NOT render in the collapsed view.
    expect(wrapper.text()).not.toContain('Context sentence.');
    expect(wrapper.text()).not.toContain('Filler.');
    // The expand toggle is present.
    expect(wrapper.find('.citation-card__expand-toggle').exists()).toBe(true);
    // The snippet class is applied.
    expect(wrapper.findAll('.citation-card__passage-text--snippet').length).toBe(2);
  });

  it('expands to the full passage with inline highlights on toggle click', async () => {
    const wrapper = mountCard({
      match: makeMatch({
        matchedPassage: 'Context. KEY SENTENCE. End.',
        highlightedSentences: ['KEY SENTENCE'],
      }),
    });
    // Initially collapsed: context not visible.
    expect(wrapper.text()).not.toContain('Context.');
    // Click the expand toggle.
    await wrapper.find('.citation-card__expand-toggle').trigger('click');
    // Expanded: full passage renders, including the context.
    expect(wrapper.text()).toContain('Context.');
    expect(wrapper.text()).toContain('KEY SENTENCE');
    expect(wrapper.text()).toContain('End.');
    // The inline <mark> highlight is applied to the key sentence.
    expect(wrapper.find('mark.citation-card__passage-mark').exists()).toBe(true);
    expect(wrapper.find('mark.citation-card__passage-mark').text()).toContain('KEY SENTENCE');
    // The toggle label flips to "Less".
    expect(wrapper.text()).toContain('Less');
  });

  it('collapses back to snippets on a second toggle click', async () => {
    const wrapper = mountCard({
      match: makeMatch({
        matchedPassage: 'Context. KEY SENTENCE. End.',
        highlightedSentences: ['KEY SENTENCE'],
      }),
    });
    // Expand.
    await wrapper.find('.citation-card__expand-toggle').trigger('click');
    expect(wrapper.text()).toContain('Context.');
    // Collapse.
    await wrapper.find('.citation-card__expand-toggle').trigger('click');
    // Context is hidden again; snippet still shows.
    expect(wrapper.text()).not.toContain('Context.');
    expect(wrapper.text()).toContain('KEY SENTENCE');
  });

  it('handles highlightedSentences not found in passage (graceful inline skip)', () => {
    // The backend grounding gate should prevent this, but the card must be
    // defensive: an un-locatable sentence is shown as a snippet (collapsed)
    // and skipped in the inline-expanded view rather than crashing.
    const wrapper = mountCard({
      match: makeMatch({
        matchedPassage: 'Real passage text.',
        highlightedSentences: ['Hallucinated sentence not in passage'],
      }),
    });
    // Collapsed: the snippet still renders (it's in highlightedSentences).
    expect(wrapper.text()).toContain('Hallucinated sentence not in passage');
    // The expand toggle is present; clicking it must not throw.
    expect(wrapper.find('.citation-card__expand-toggle').exists()).toBe(true);
  });
});
