import { describe, it, expect } from 'vitest';
import { highlightSearchTerms, extractSearchTerms, HIGHLIGHT_CLASS } from '@/utils/highlight';

const MARK_OPEN = `<mark class="${HIGHLIGHT_CLASS}">`;

describe('highlightSearchTerms', () => {
  it('wraps a simple match in a mark tag', () => {
    const result = highlightSearchTerms('the sugar tax levy', 'sugar');
    expect(result).toBe(`the ${MARK_OPEN}sugar</mark> tax levy`);
  });

  it('is case-insensitive', () => {
    const result = highlightSearchTerms('Sugar TAX levy', 'tax');
    expect(result).toBe(`Sugar ${MARK_OPEN}TAX</mark> levy`);
  });

  it('wraps multiple occurrences', () => {
    const result = highlightSearchTerms('sugar sugar sugar', 'sugar');
    expect(result).toBe(
      `${MARK_OPEN}sugar</mark> ${MARK_OPEN}sugar</mark> ${MARK_OPEN}sugar</mark>`
    );
  });

  it('wraps multiple terms (alternation)', () => {
    const result = highlightSearchTerms('sugar and obesity', 'sugar obesity');
    expect(result).toBe(`${MARK_OPEN}sugar</mark> and ${MARK_OPEN}obesity</mark>`);
  });

  it('returns input unchanged when query is empty', () => {
    const html = '<p>no changes</p>';
    expect(highlightSearchTerms(html, '')).toBe(html);
    expect(highlightSearchTerms(html, '   ')).toBe(html);
  });

  it('drops single-character terms (too noisy)', () => {
    const html = '<p>a b c sugar</p>';
    const result = highlightSearchTerms(html, 'a b c');
    // All terms are single chars → no highlighting.
    expect(result).toBe(html);
  });

  it('keeps qualifying terms and drops single chars in a mixed query', () => {
    const result = highlightSearchTerms('a sugar b tax', 'a sugar b tax');
    expect(result).toBe(`a ${MARK_OPEN}sugar</mark> b ${MARK_OPEN}tax</mark>`);
  });

  it('does not touch tag segments (markup untouched)', () => {
    const html = '<a href="/sugar">sugar</a>';
    const result = highlightSearchTerms(html, 'sugar');
    // The href="/sugar" must NOT be altered; only the inner text.
    expect(result).toBe(`<a href="/sugar">${MARK_OPEN}sugar</mark></a>`);
  });

  it('does not touch attributes that contain the term', () => {
    const html = '<span data-slug="sugar-tax">sugar</span>';
    const result = highlightSearchTerms(html, 'sugar');
    expect(result).toBe(`<span data-slug="sugar-tax">${MARK_OPEN}sugar</mark></span>`);
  });

  it('escapes regex special characters in the term', () => {
    // A term with regex metacharacters should be treated literally.
    const result = highlightSearchTerms('price: $9.99 each', '$9.99');
    expect(result).toBe(`price: ${MARK_OPEN}$9.99</mark> each`);
  });

  it('handles nested tags', () => {
    const html = '<ul><li>sugar</li><li>tax</li></ul>';
    const result = highlightSearchTerms(html, 'sugar tax');
    expect(result).toBe(
      `<ul><li>${MARK_OPEN}sugar</mark></li><li>${MARK_OPEN}tax</mark></li></ul>`
    );
  });

  it('handles HTML entities in text segments', () => {
    // `&` in text stays intact; the term matches the literal text around it.
    const html = '<p>sugar & spice</p>';
    const result = highlightSearchTerms(html, 'sugar');
    expect(result).toBe(`<p>${MARK_OPEN}sugar</mark> & spice</p>`);
  });
});

describe('extractSearchTerms', () => {
  it('splits on whitespace and filters short tokens', () => {
    expect(extractSearchTerms('the sugar tax')).toEqual(['the', 'sugar', 'tax']);
  });

  it('drops single-char tokens', () => {
    expect(extractSearchTerms('a b sugar')).toEqual(['sugar']);
  });

  it('returns empty for empty / whitespace query', () => {
    expect(extractSearchTerms('')).toEqual([]);
    expect(extractSearchTerms('   ')).toEqual([]);
  });

  it('drops punctuation-only tokens', () => {
    expect(extractSearchTerms('!!! ??? sugar')).toEqual(['sugar']);
  });

  it('keeps tokens with alphanumeric chars', () => {
    expect(extractSearchTerms('v1 2.0 sugar')).toEqual(['v1', '2.0', 'sugar']);
  });
});
