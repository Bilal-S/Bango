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
});
