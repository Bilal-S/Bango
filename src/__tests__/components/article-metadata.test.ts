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
    // Journal label may contain a nested "(unrecognized)" span, so use
    // `includes` rather than exact match.
    const journalLabel = labels.find((s) => s.text().includes('Journal'));
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
    const journalLabel = labels.find((s) => s.text().includes('Journal'));
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

  // ── Always-show placeholders (empty fields render `---`) ───────────

  it('renders --- placeholder for empty DOI instead of hiding it', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ doi: null }) },
    });
    expect(wrapper.text()).toContain('DOI');
    expect(wrapper.text()).toContain('---');
    // No anchor link should render when DOI is null.
    expect(wrapper.find('a').exists()).toBe(false);
  });

  it('renders --- placeholder for empty keywords instead of hiding it', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ keywords: [] }) },
    });
    expect(wrapper.text()).toContain('Keywords');
    expect(wrapper.text()).toContain('---');
  });

  it('renders --- placeholder for empty authors instead of hiding it', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ authors: [] }) },
    });
    expect(wrapper.text()).toContain('Authors');
    expect(wrapper.text()).toContain('---');
  });

  it('renders --- placeholder for empty affiliation instead of hiding it', () => {
    const wrapper = mount(ArticleMetadata, {
      props: { article: makeArticle({ affiliation: null }) },
    });
    expect(wrapper.text()).toContain('Affiliation');
    expect(wrapper.text()).toContain('---');
  });

  // ── Inline editing ─────────────────────────────────────────────────

  it('shows an edit input on double-click of the Journal value', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    // No input before edit.
    expect(wrapper.find('.meta-edit-input').exists()).toBe(false);
    // Find the Journal value span (has the @dblclick handler).
    const journalSpans = wrapper.findAll('span.cursor-text');
    const journalSpan = journalSpans.find((s) => s.text() === 'Nature');
    expect(journalSpan).toBeTruthy();
    await journalSpan!.trigger('dblclick');
    // Input should now be visible.
    const input = wrapper.find('.meta-edit-input');
    expect(input.exists()).toBe(true);
    expect((input.element as HTMLInputElement).value).toBe('Nature');
  });

  it('emits updateField with field + value when committing a Journal edit', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const journalSpans = wrapper.findAll('span.cursor-text');
    const journalSpan = journalSpans.find((s) => s.text() === 'Nature');
    await journalSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('Science');
    await input.trigger('blur');
    const emitted = wrapper.emitted('updateField');
    expect(emitted).toBeTruthy();
    expect(emitted![0]![0]).toBe('journal');
    expect(emitted![0]![1]).toBe('Science');
  });

  it('emits updateField with an array for Authors (comma-separated)', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const authorSpans = wrapper.findAll('span.cursor-text');
    const authorSpan = authorSpans.find((s) => s.text().includes('Smith, J.'));
    await authorSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('Doe A, Roe B');
    await input.trigger('blur');
    const emitted = wrapper.emitted('updateField');
    expect(emitted).toBeTruthy();
    expect(emitted![0]![0]).toBe('authors');
    expect(emitted![0]![1]).toEqual(['Doe A', 'Roe B']);
  });

  it('does NOT emit updateField when the value is unchanged', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const journalSpans = wrapper.findAll('span.cursor-text');
    const journalSpan = journalSpans.find((s) => s.text() === 'Nature');
    await journalSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    // Value is pre-seeded with the current value.
    await input.trigger('blur');
    expect(wrapper.emitted('updateField')).toBeFalsy();
  });

  it('cancels the edit on Escape without emitting', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const journalSpans = wrapper.findAll('span.cursor-text');
    const journalSpan = journalSpans.find((s) => s.text() === 'Nature');
    await journalSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('Changed');
    await input.trigger('keyup.escape');
    expect(wrapper.emitted('updateField')).toBeFalsy();
    // Input is gone; value span is back.
    expect(wrapper.find('.meta-edit-input').exists()).toBe(false);
  });

  it('clears the field when committing an empty value (scalar)', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const journalSpans = wrapper.findAll('span.cursor-text');
    const journalSpan = journalSpans.find((s) => s.text() === 'Nature');
    await journalSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('   ');
    await input.trigger('blur');
    const emitted = wrapper.emitted('updateField');
    expect(emitted).toBeTruthy();
    // Trimmed to empty -> backend clears to NULL.
    expect(emitted![0]![1]).toBe('');
  });

  // ── Year validation (1800–2100, 4-digit) ───────────────────────────

  it('shows a validation error when Year is out of range (1799)', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const yearSpans = wrapper.findAll('span.cursor-text');
    const yearSpan = yearSpans.find((s) => s.text() === '2021');
    await yearSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('1799');
    await input.trigger('blur');
    // Should NOT emit (blocked by validation); shows error.
    expect(wrapper.emitted('updateField')).toBeFalsy();
    expect(wrapper.text()).toContain('1800');
  });

  it('shows a validation error when Year is out of range (2101)', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const yearSpans = wrapper.findAll('span.cursor-text');
    const yearSpan = yearSpans.find((s) => s.text() === '2021');
    await yearSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('2101');
    await input.trigger('blur');
    expect(wrapper.emitted('updateField')).toBeFalsy();
  });

  it('shows a validation error when Year is non-numeric', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const yearSpans = wrapper.findAll('span.cursor-text');
    const yearSpan = yearSpans.find((s) => s.text() === '2021');
    await yearSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('abcd');
    await input.trigger('blur');
    expect(wrapper.emitted('updateField')).toBeFalsy();
    expect(wrapper.text()).toContain('4-digit');
  });

  it('accepts a valid in-range Year and emits', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const yearSpans = wrapper.findAll('span.cursor-text');
    const yearSpan = yearSpans.find((s) => s.text() === '2021');
    await yearSpan!.trigger('dblclick');
    const input = wrapper.find('.meta-edit-input');
    await input.setValue('1999');
    await input.trigger('blur');
    const emitted = wrapper.emitted('updateField');
    expect(emitted).toBeTruthy();
    expect(emitted![0]![1]).toBe('1999');
  });

  // ── Journal recognition indicator ──────────────────────────────────

  it('shows "(unrecognized)" when journal is set but journalIndexId is null', () => {
    const wrapper = mount(ArticleMetadata, {
      props: {
        article: makeArticle({ journal: 'Mystery Journal', journalIndexId: null }),
      },
    });
    expect(wrapper.text()).toContain('(unrecognized)');
  });

  it('does NOT show "(unrecognized)" when journalIndexId is set', () => {
    const wrapper = mount(ArticleMetadata, {
      props: {
        article: makeArticle({ journal: 'Nature', journalIndexId: 'idx-1' }),
      },
    });
    expect(wrapper.text()).not.toContain('(unrecognized)');
  });

  it('does NOT show "(unrecognized)" when journal is empty', () => {
    const wrapper = mount(ArticleMetadata, {
      props: {
        article: makeArticle({ journal: null, journalIndexId: null }),
      },
    });
    expect(wrapper.text()).not.toContain('(unrecognized)');
  });

  // ── Language dropdown ──────────────────────────────────────────────

  it('renders a <select> dropdown when editing the language field', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    // Find the Lang value span by its parent label.
    const allSpans = wrapper.findAll('span.cursor-text');
    const langValueSpan = allSpans.find((s) => {
      // The language display is '---' when null; our fixture has language: null.
      const parent = s.element.parentElement;
      return parent?.textContent?.includes('Lang') === true;
    });
    await langValueSpan!.trigger('dblclick');
    // Should now show a <select>, not a text <input>.
    expect(wrapper.find('select.meta-edit-input').exists()).toBe(true);
    expect(wrapper.find('input.meta-edit-input').exists()).toBe(false);
  });

  it('language dropdown includes the curated list + "Other…" option', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const allSpans = wrapper.findAll('span.cursor-text');
    const langValueSpan = allSpans.find((s) => {
      const parent = s.element.parentElement;
      return parent?.textContent?.includes('Lang') === true;
    });
    await langValueSpan!.trigger('dblclick');
    const select = wrapper.find('select.meta-edit-input');
    const options = select.findAll('option');
    const optionTexts = options.map((o) => o.text());
    expect(optionTexts).toContain('English');
    expect(optionTexts).toContain('French');
    expect(optionTexts).toContain('Chinese');
    expect(optionTexts).toContain('Other…');
  });

  it('emits updateField with language when a language option is selected', async () => {
    const wrapper = mount(ArticleMetadata, { props: { article: makeArticle() } });
    const allSpans = wrapper.findAll('span.cursor-text');
    const langValueSpan = allSpans.find((s) => {
      const parent = s.element.parentElement;
      return parent?.textContent?.includes('Lang') === true;
    });
    await langValueSpan!.trigger('dblclick');

    const select = wrapper.find('select.meta-edit-input');
    // Simulate the user selecting French via the native dropdown.
    await select.setValue('French');

    const emitted = wrapper.emitted('updateField');
    expect(emitted).toBeTruthy();
    expect(emitted![0]![0]).toBe('language');
    expect(emitted![0]![1]).toBe('French');
  });
});
