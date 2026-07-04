import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DetailHeader from '@/components/detail-header.vue';
import type { Article } from '@/types';

/**
 * Article factory for the translation test suite. Includes the Plan-A
 * translation fields so the full Article shape is satisfied once the
 * implementation lands.
 */
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

const baseProps = {
  article: makeArticle(),
  canRequestAiSummary: false,
  isAiSummaryPending: false,
  hasReturnTarget: false,
  fullScreen: false,
};

describe('detail-header.vue - translation (language-plan-v2)', () => {
  it('shows_translate_button_for_non_english_not_translated', () => {
    // TC-09: translate icon button renders for a non-English, not-yet-translated article.
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ language: 'French', isTranslated: false }),
      },
    });
    // The Material Symbol `translate` button is rendered.
    const translateBtn = wrapper.find('button[title="Translate to English"]');
    expect(translateBtn.exists()).toBe(true);
    expect(translateBtn.text()).toContain('translate');
  });

  it('hides_translate_button_when_translated', () => {
    // TC-09: once isTranslated is true, the translate button is replaced by the
    // green "Translated" status chip; the action button does NOT render.
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ language: 'French', isTranslated: true }),
      },
    });
    const translateBtn = wrapper.find('button[title="Translate to English"]');
    expect(translateBtn.exists()).toBe(false);
    // The translated status chip is rendered instead.
    expect(wrapper.html()).toContain('Translated');
  });
});
