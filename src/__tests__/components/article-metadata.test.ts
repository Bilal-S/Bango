import { describe, it, expect, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import ArticleMetadata from '@/components/article-metadata.vue';
import type { Article } from '@/types';

function makeArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: 'a1',
    sequenceId: 1,
    status: 'included',
    screeningError: false,
    title: 'T',
    abstractText: '',
    authors: ['Smith, J.'],
    publicationYear: 2021,
    doi: '10.1000/foo',
    journal: 'Nature',
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: ['sugar', 'tax'],
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
    affiliation: 'Harvard',
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

// happy-dom's localStorage lacks removeItem/clear; provide a minimal shim
// that supports getItem/setItem used by the component.
function shimLocalStorage() {
  const store = new Map<string, string>();
  return {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => store.set(k, v),
    removeItem: (k: string) => {
      store.delete(k);
    },
    clear: () => store.clear(),
  };
}

describe('article-metadata.vue', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'localStorage', {
      value: shimLocalStorage(),
      configurable: true,
    });
  });

  it('renders Metadata header and expand icon', () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    expect(wrapper.text()).toContain('Metadata');
    expect(wrapper.text()).toContain('expand_more');
  });

  it('renders authors when expanded', () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    expect(wrapper.text()).toContain('Authors');
    expect(wrapper.text()).toContain('Smith, J.');
  });

  it('renders journal and year', () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    expect(wrapper.text()).toContain('Journal');
    expect(wrapper.text()).toContain('Nature');
    expect(wrapper.text()).toContain('Year');
    expect(wrapper.text()).toContain('2021');
  });

  it('renders DOI link when present', () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const link = wrapper.find('a');
    expect(link.attributes('href')).toBe('https://doi.org/10.1000/foo');
  });

  it('renders keywords when present', () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    expect(wrapper.text()).toContain('Keywords');
    expect(wrapper.text()).toContain('sugar');
    expect(wrapper.text()).toContain('tax');
  });

  it('renders affiliation when present', () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    expect(wrapper.text()).toContain('Affiliation');
    expect(wrapper.text()).toContain('Harvard');
  });

  it('renders dashes for missing journal and year', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ journal: null, publicationYear: null }) },
    });
    expect(wrapper.text()).toContain('---');
  });

  it('renders Lang label and language value when present', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ language: 'French' }) },
    });
    expect(wrapper.text()).toContain('Lang');
    expect(wrapper.text()).toContain('French');
  });

  it('renders dashes for missing language', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ language: null }) },
    });
    expect(wrapper.text()).toContain('Lang');
    // Language is null -> the `?? '---'` fallback renders dashes.
    expect(wrapper.text()).toContain('---');
  });

  it('sets title attribute on Journal value span so the full name shows on hover', () => {
    // When the journal name is truncated by the `truncate` class, the native
    // tooltip (title attribute) carries the full name so the user can still
    // read it on hover. The Journal label span is followed by the value span.
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ journal: 'Journal of Long Name Example' }) },
    });
    const labels = wrapper.findAll('span.text-slate-500');
    const journalLabel = labels.find((s) => s.text() === 'Journal');
    expect(journalLabel).toBeTruthy();
    const valueSpan = journalLabel!.element.nextElementSibling as HTMLElement;
    expect(valueSpan).toBeTruthy();
    expect(valueSpan.getAttribute('title')).toBe('Journal of Long Name Example');
  });

  it('sets empty title on Journal value span when journal is null', () => {
    // No stray tooltip should appear for the `---` placeholder.
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ journal: null }) },
    });
    const labels = wrapper.findAll('span.text-slate-500');
    const journalLabel = labels.find((s) => s.text() === 'Journal');
    expect(journalLabel).toBeTruthy();
    const valueSpan = journalLabel!.element.nextElementSibling as HTMLElement;
    expect(valueSpan).toBeTruthy();
    expect(valueSpan.getAttribute('title')).toBe('');
  });

  it('toggles metadata expanded state and persists to localStorage', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const button = wrapper.find('button');
    await button.trigger('click');
    expect(localStorage.getItem('bango-metadata-expanded')).toBe('false');
    await button.trigger('click');
    expect(localStorage.getItem('bango-metadata-expanded')).toBe('true');
  });

  it('renders compact author preview in header when collapsed', async () => {
    localStorage.setItem('bango-metadata-expanded', 'false');
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    // Header preview shows authors when collapsed - check the toggle button text
    const toggleButton = wrapper.find('button');
    expect(toggleButton.text()).toContain('Smith, J.');
  });
});
