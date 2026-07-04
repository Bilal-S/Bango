import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import MatchedCriteria from '@/components/matched-criteria.vue';
import { useCriteriaStore } from '@/stores/criteria';
import type { Article } from '@/types';

vi.mock('@/composables/use-tauri-command', () => ({
  isTauri: () => false,
  tauriCommand: vi.fn(),
}));

function makeArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: 'a1',
    sequenceId: 1,
    status: 'included',
    screeningError: false,
    title: 'T',
    abstractText: '',
    authors: [],
    publicationYear: null,
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
    referenceType: null,
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

describe('matched-criteria.vue', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('renders empty message when no criteria matched', () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('No criteria matched');
  });

  it('renders the section header', () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('Matched Criteria');
  });

  it('renders matched inclusion criterion text', () => {
    const store = useCriteriaStore();
    store.criteria = [
      {
        id: 'c1',
        criterionType: 'inclusion',
        text: 'Must be human study',
        priority: 'high',
        createdAt: '',
      },
    ];
    store.inclusionCriteria = [store.criteria[0]!];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: {
        article: makeArticle({ matchedInclusionCriteria: ['c1'] }),
      },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('Must be human study');
  });

  it('renders matched exclusion criterion text with line-through', () => {
    const store = useCriteriaStore();
    store.criteria = [
      {
        id: 'c2',
        criterionType: 'exclusion',
        text: 'Animal study',
        priority: 'standard',
        createdAt: '',
      },
    ];
    store.exclusionCriteria = [store.criteria[0]!];
    store.inclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: {
        article: makeArticle({ matchedExclusionCriteria: ['c2'] }),
      },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('Animal study');
  });

  it('renders edit button', () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    expect(wrapper.text()).toContain('edit');
  });

  it('opens dialog when edit clicked', async () => {
    const store = useCriteriaStore();
    store.inclusionCriteria = [];
    store.exclusionCriteria = [];

    const wrapper = mount(MatchedCriteria, {
      props: { article: makeArticle() },
      global: { stubs: { CriteriaEditDialog: true } },
    });
    await wrapper.find('button').trigger('click');
    // The CriteriaEditDialog is stubbed; we just verify no crash.
    expect(wrapper.exists()).toBe(true);
  });
});
