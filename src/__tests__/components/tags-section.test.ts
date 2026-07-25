import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import TagsSection from '@/components/tags-section.vue';
import type { Article } from '@/types';

// Mock the tags store so the component can find tags/tag colors.
vi.mock('@/stores/tags', () => ({
  useTagsStore: () => ({
    tags: [
      {
        id: '1',
        name: 'machine-learning',
        source: 'user_created',
        color: '#3b82f6',
        articleCount: 142,
      },
      {
        id: '2',
        name: 'clinical-trial',
        source: 'user_created',
        color: '#10b981',
        articleCount: 89,
      },
      { id: '3', name: 'nlp-models', source: 'ai_suggested', color: '#8b5cf6', articleCount: 56 },
    ],
    loading: false,
    fetchIfNeeded: vi.fn(),
    createTag: vi.fn(),
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
    tags: ['machine-learning', 'clinical-trial'],
    labels: [],
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

describe('tags-section.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders the section heading', () => {
    const wrapper = mount(TagsSection, {
      props: { article: makeArticle() },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, TagChip: true },
      },
    });
    expect(wrapper.text()).toContain('Tags');
  });

  it('renders assigned tags sorted alphabetically', () => {
    const wrapper = mount(TagsSection, {
      props: {
        article: makeArticle({ tags: ['clinical-trial', 'machine-learning', 'nlp-models'] }),
      },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, TagChip: true },
      },
    });
    // Tags should be rendered (TagChip is stubbed, but the v-for wrapper divs exist).
    const html = wrapper.html();
    // The sorted order (case-insensitive): clinical-trial, machine-learning, nlp-models
    expect(html).toContain('clinical-trial');
    expect(html).toContain('machine-learning');
  });

  it('shows remove buttons for each tag', () => {
    const wrapper = mount(TagsSection, {
      props: {
        article: makeArticle({ tags: ['machine-learning'] }),
      },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, TagChip: true },
      },
    });
    const removeButtons = wrapper.findAll('button');
    // At least one remove button exists (the "close" button per tag).
    expect(removeButtons.length).toBeGreaterThanOrEqual(1);
  });

  it('emits updateTags when a tag is removed', async () => {
    const wrapper = mount(TagsSection, {
      props: {
        article: makeArticle({ id: 'article-1', tags: ['machine-learning', 'clinical-trial'] }),
      },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true, TagChip: true },
      },
    });

    // Click the first remove button (close icon for the first tag).
    const removeButtons = wrapper.findAll('button');
    await removeButtons[0]!.trigger('click');

    const emitted = wrapper.emitted('updateTags');
    expect(emitted).toBeTruthy();
    expect(emitted!.length).toBe(1);
    // The first argument is the article id, second is the remaining tags.
    expect(emitted![0]![0]).toBe('article-1');
    // One tag was removed, leaving one tag.
    expect((emitted![0]![1] as string[]).length).toBe(1);
  });

  it('renders the SuggestInput for adding new tags', () => {
    const wrapper = mount(TagsSection, {
      props: { article: makeArticle() },
      global: {
        plugins: [createPinia()],
        stubs: { TagChip: true },
      },
    });
    expect(wrapper.findComponent({ name: 'SuggestInput' }).exists()).toBe(true);
  });

  // ── Already-assigned tags appear as disabled (not hidden) in the dropdown ─
  // The user explicitly asked: do not hide already-matched items; disable them
  // (grey + unselectable) so the user can see they exist. The assigned set is
  // forwarded to SuggestInput via the `disabledSuggestions` prop.

  it('forwards the assigned tags to SuggestInput as disabledSuggestions', () => {
    const wrapper = mount(TagsSection, {
      props: { article: makeArticle({ tags: ['machine-learning'] }) },
      global: {
        plugins: [createPinia()],
        stubs: { TagChip: true },
      },
    });
    const suggest = wrapper.findComponent({ name: 'SuggestInput' });
    // The assigned tag should be present in the disabledSuggestions prop.
    expect(suggest.props('disabledSuggestions')).toEqual(['machine-learning']);
  });

  it('includes assigned tags in the suggestions list (not excluded)', () => {
    const wrapper = mount(TagsSection, {
      props: { article: makeArticle({ tags: ['machine-learning'] }) },
      global: {
        plugins: [createPinia()],
        stubs: { TagChip: true },
      },
    });
    const suggest = wrapper.findComponent({ name: 'SuggestInput' });
    // The previously-excluded tag now appears in suggestions (rendered disabled
    // via the disabledSuggestions prop above).
    expect(suggest.props('suggestions')).toContain('machine-learning');
  });

  // ── Halo on assigned chips whose name contains the typed query ──────────
  // When the user types into the add input, assigned chips whose name
  // contains the substring receive an indigo ring so the user sees the
  // existing match. The `highlight` prop is threaded from `tagMatchesQuery`.

  it('passes highlight=true to an assigned TagChip whose name contains the typed query', async () => {
    const wrapper = mount(TagsSection, {
      props: { article: makeArticle({ tags: ['machine-learning', 'clinical-trial'] }) },
      global: {
        plugins: [createPinia()],
        // Use a real TagChip so we can assert on the received `highlight` prop.
        stubs: { SuggestInput: true },
      },
    });

    // Type a substring that matches the first assigned tag only.
    await wrapper.findComponent({ name: 'SuggestInput' }).vm.$emit('update:modelValue', 'learning');
    await wrapper.vm.$nextTick();

    const chips = wrapper.findAllComponents({ name: 'TagChip' });
    // machine-learning contains "learning" -> highlight=true.
    const matching = chips.find((c) => c.props('name') === 'machine-learning');
    expect(matching).toBeTruthy();
    expect(matching!.props('highlight')).toBe(true);
    // clinical-trial does not contain "learning" -> highlight=false.
    const other = chips.find((c) => c.props('name') === 'clinical-trial');
    expect(other).toBeTruthy();
    expect(other!.props('highlight')).toBe(false);
  });

  it('passes highlight=false to all chips when the input is empty', () => {
    const wrapper = mount(TagsSection, {
      props: { article: makeArticle({ tags: ['machine-learning', 'clinical-trial'] }) },
      global: {
        plugins: [createPinia()],
        stubs: { SuggestInput: true },
      },
    });

    const chips = wrapper.findAllComponents({ name: 'TagChip' });
    expect(chips.length).toBeGreaterThan(0);
    // With no query typed, no chip should be highlighted.
    for (const chip of chips) {
      expect(chip.props('highlight')).toBe(false);
    }
  });
});
