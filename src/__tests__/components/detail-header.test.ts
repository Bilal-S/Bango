import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DetailHeader from '@/components/detail-header.vue';
import type { Article } from '@/types';

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

describe('detail-header.vue', () => {
  it('renders the article title', () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    expect(wrapper.text()).toContain('Test Article');
  });

  it('renders publication-type label', () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    expect(wrapper.text()).toContain('JOURNAL');
  });

  it('shows close icon when no return target', () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    expect(wrapper.text()).toContain('close');
  });

  it('shows back arrow when hasReturnTarget is true', () => {
    const wrapper = mount(DetailHeader, {
      props: { ...baseProps, hasReturnTarget: true },
    });
    expect(wrapper.text()).toContain('arrow_back');
  });

  it('shows open_in_full when not fullscreen', () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    expect(wrapper.text()).toContain('open_in_full');
  });

  it('shows close_fullscreen when fullscreen', () => {
    const wrapper = mount(DetailHeader, {
      props: { ...baseProps, fullScreen: true },
    });
    expect(wrapper.text()).toContain('close_fullscreen');
  });

  it('shows attach_file button when no full text attached', () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    expect(wrapper.text()).toContain('attach_file');
  });

  it('shows full-text icon when attached', () => {
    const wrapper = mount(DetailHeader, {
      props: {
        ...baseProps,
        article: makeArticle({ hasFullText: true, fullTextFileName: 'paper.pdf' }),
        hasFiguresOrTables: false,
        fullTextFileIcon: 'picture_as_pdf',
      },
    });
    expect(wrapper.text()).toContain('picture_as_pdf');
    expect(wrapper.text()).not.toContain('attach_file');
  });

  it('emits close when close button clicked', async () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    const buttons = wrapper.findAll('button');
    const closeBtn = buttons[buttons.length - 1]!;
    await closeBtn.trigger('click');
    expect(wrapper.emitted('close')).toBeTruthy();
  });

  it('emits toggleFullScreen when fullscreen button clicked', async () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    const buttons = wrapper.findAll('button');
    // toggle button is the second-to-last
    const toggleBtn = buttons[buttons.length - 2]!;
    await toggleBtn.trigger('click');
    expect(wrapper.emitted('toggleFullScreen')).toBeTruthy();
  });

  it('emits attachFullText with article id when attach clicked', async () => {
    const wrapper = mount(DetailHeader, { props: baseProps });
    const attachBtn = wrapper.findAll('button')[0]!;
    await attachBtn.trigger('click');
    const events = wrapper.emitted('attachFullText');
    expect(events).toBeTruthy();
    expect(events![0]).toEqual(['a1']);
  });

  it('shows AI summary button when canRequestAiSummary', () => {
    const wrapper = mount(DetailHeader, {
      props: { ...baseProps, canRequestAiSummary: true },
    });
    expect(wrapper.text()).toContain('auto_awesome');
  });

  it('shows pending spinner when isAiSummaryPending and not canRequest', () => {
    const wrapper = mount(DetailHeader, {
      props: { ...baseProps, isAiSummaryPending: true },
    });
    expect(wrapper.text()).toContain('progress_activity');
  });
});
