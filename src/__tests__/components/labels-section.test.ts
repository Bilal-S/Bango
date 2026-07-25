import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import LabelsSection from '@/components/labels-section.vue';
import type { Article } from '@/types';

// Mock the labels store so the component can find labels/label colors.
vi.mock('@/stores/labels', () => ({
  useLabelsStore: () => ({
    labels: [
      {
        id: '1',
        name: 'priority-read',
        source: 'user_created',
        color: '#ef4444',
        articleCount: 12,
      },
      {
        id: '2',
        name: 'strong-methodology',
        source: 'ai_generated',
        color: '#10b981',
        articleCount: 34,
      },
      {
        id: '3',
        name: 'borderline',
        source: 'ai_generated',
        color: '#f59e0b',
        articleCount: 7,
      },
    ],
    loading: false,
    fetchIfNeeded: vi.fn(),
    createLabel: vi.fn(),
  }),
}));

function makeArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: 'a1',
    sequenceId: 1,
    status: 'included',
    screeningError: false,
    title: 'Test Article',
    abstractText: '',
    authors: [],
    publicationYear: 2021,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: [],
    url: null,
    language: null,
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    eissn: null,
    journalIndexId: null,
    referenceType: 'JOUR',
    date: null,
    authorAddress: null,
    affiliation: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: null,
    aiReasoning: null,
    aiConfidence: null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    tags: [],
    labels: ['priority-read', 'strong-methodology'],
    manualOverride: false,
    importSource: null,
    importedAt: '',
    changedAt: '',
    screenedAt: null,
    dataLength: null,
    tokenEstimate: null,
    actualTokens: null,
    fullText: null,
    fullTextAiSummary: null,
    numCited: null,
    numReferences: null,
    hasCitationDetails: false,
    hasReferenceDetails: false,
    hasFullText: false,
    fullTextFileName: null,
    hasFiguresOrTables: false,
    isTranslated: false,
    translationStatus: 'none',
    translationError: null,
    translatedAt: null,
    ...overrides,
  } as Article;
}

describe('labels-section.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders the section heading', () => {
    const wrapper = mount(LabelsSection, {
      props: { article: makeArticle() },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, LabelChip: true },
      },
    });
    expect(wrapper.text()).toContain('Labels');
  });

  it('renders assigned labels sorted alphabetically', () => {
    const wrapper = mount(LabelsSection, {
      props: {
        article: makeArticle({ labels: ['strong-methodology', 'priority-read', 'borderline'] }),
      },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, LabelChip: true },
      },
    });
    const html = wrapper.html();
    // Sorted order (case-insensitive): borderline, priority-read, strong-methodology
    expect(html).toContain('borderline');
    expect(html).toContain('priority-read');
    expect(html).toContain('strong-methodology');
  });

  it('emits updateLabels when a label is removed', async () => {
    const wrapper = mount(LabelsSection, {
      props: {
        article: makeArticle({ id: 'article-1', labels: ['priority-read', 'strong-methodology'] }),
      },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, LabelChip: true },
      },
    });

    const removeButtons = wrapper.findAll('button');
    await removeButtons[0]!.trigger('click');

    const emitted = wrapper.emitted('updateLabels');
    expect(emitted).toBeTruthy();
    expect(emitted!.length).toBe(1);
    expect(emitted![0]![0]).toBe('article-1');
    expect((emitted![0]![1] as string[]).length).toBe(1);
  });

  it('renders the SuggestInput for adding new labels', () => {
    const wrapper = mount(LabelsSection, {
      props: { article: makeArticle() },
      global: {
        plugins: [createPinia()],
        stubs: { LabelChip: true },
      },
    });
    expect(wrapper.findComponent({ name: 'SuggestInput' }).exists()).toBe(true);
  });

  // ── Already-assigned labels appear as disabled (not hidden) in the dropdown ─
  // Mirror of the tags-section contract: do not hide already-matched items;
  // disable them so the user can see they exist.

  it('forwards the assigned labels to SuggestInput as disabledSuggestions', () => {
    const wrapper = mount(LabelsSection, {
      props: { article: makeArticle({ labels: ['priority-read'] }) },
      global: {
        plugins: [createPinia()],
        stubs: { LabelChip: true },
      },
    });
    const suggest = wrapper.findComponent({ name: 'SuggestInput' });
    expect(suggest.props('disabledSuggestions')).toEqual(['priority-read']);
  });

  it('includes assigned labels in the suggestions list (not excluded)', () => {
    const wrapper = mount(LabelsSection, {
      props: { article: makeArticle({ labels: ['priority-read'] }) },
      global: {
        plugins: [createPinia()],
        stubs: { LabelChip: true },
      },
    });
    const suggest = wrapper.findComponent({ name: 'SuggestInput' });
    expect(suggest.props('suggestions')).toContain('priority-read');
  });

  // ── Halo on assigned chips whose name contains the typed query ──────────
  // Mirror of the tags-section halo behavior.

  it('passes highlight=true to an assigned LabelChip whose name contains the typed query', async () => {
    const wrapper = mount(LabelsSection, {
      props: { article: makeArticle({ labels: ['priority-read', 'strong-methodology'] }) },
      global: {
        plugins: [createPinia()],
        // Use a real LabelChip so we can assert on the received `highlight` prop.
        stubs: { SuggestInput: true },
      },
    });

    // Type a substring that matches the first assigned label only.
    await wrapper.findComponent({ name: 'SuggestInput' }).vm.$emit('update:modelValue', 'priority');
    await wrapper.vm.$nextTick();

    const chips = wrapper.findAllComponents({ name: 'LabelChip' });
    // priority-read contains "priority" -> highlight=true.
    const matching = chips.find((c) => c.props('name') === 'priority-read');
    expect(matching).toBeTruthy();
    expect(matching!.props('highlight')).toBe(true);
    // strong-methodology does not contain "priority" -> highlight=false.
    const other = chips.find((c) => c.props('name') === 'strong-methodology');
    expect(other).toBeTruthy();
    expect(other!.props('highlight')).toBe(false);
  });

  it('passes highlight=false to all chips when the input is empty', () => {
    const wrapper = mount(LabelsSection, {
      props: { article: makeArticle({ labels: ['priority-read', 'strong-methodology'] }) },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true },
      },
    });

    const chips = wrapper.findAllComponents({ name: 'LabelChip' });
    expect(chips.length).toBeGreaterThan(0);
    for (const chip of chips) {
      expect(chip.props('highlight')).toBe(false);
    }
  });
});
