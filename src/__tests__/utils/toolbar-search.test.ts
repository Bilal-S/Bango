import { describe, it, expect } from 'vitest';
import { parseToolbarSearch, TOOLBAR_YEAR_MIN, TOOLBAR_YEAR_MAX } from '@/utils/toolbar-search';

describe('parseToolbarSearch', () => {
  // ── Plain text ──────────────────────────────────────────────────────
  it('treats unprefixed text as a plain search', () => {
    expect(parseToolbarSearch('machine learning')).toEqual({
      kind: 'plain',
      search: 'machine learning',
    });
  });

  it('trims surrounding whitespace from a plain search', () => {
    expect(parseToolbarSearch('  alpha  ')).toEqual({ kind: 'plain', search: 'alpha' });
  });

  it('returns an empty plain search for blank text', () => {
    expect(parseToolbarSearch('   ')).toEqual({ kind: 'plain', search: '' });
  });

  it('does not treat a mid-string prefix as a field search', () => {
    // `foo a:bar` - the prefix only counts at the very start.
    expect(parseToolbarSearch('foo a:bar')).toEqual({ kind: 'plain', search: 'foo a:bar' });
  });

  it('treats a bare prefix with no value as an empty plain search', () => {
    expect(parseToolbarSearch('a:')).toEqual({ kind: 'plain', search: '' });
    expect(parseToolbarSearch('A:   ')).toEqual({ kind: 'plain', search: '' });
  });

  // ── Author ──────────────────────────────────────────────────────────
  it('parses a: as an author search', () => {
    expect(parseToolbarSearch('a:fer')).toEqual({ kind: 'author', author: 'fer' });
  });

  it('parses A: case-insensitively', () => {
    expect(parseToolbarSearch('A:fer')).toEqual({ kind: 'author', author: 'fer' });
  });

  it('keeps spaces inside the author fragment', () => {
    expect(parseToolbarSearch('a:fer nan')).toEqual({ kind: 'author', author: 'fer nan' });
  });

  it('trims the author fragment', () => {
    expect(parseToolbarSearch('  a:fer  ')).toEqual({ kind: 'author', author: 'fer' });
  });

  // ── DOI ─────────────────────────────────────────────────────────────
  it('parses d: as a DOI search', () => {
    expect(parseToolbarSearch('d:10.1001/art1')).toEqual({ kind: 'doi', doi: '10.1001/art1' });
  });

  it('parses D: case-insensitively', () => {
    expect(parseToolbarSearch('D:10.5')).toEqual({ kind: 'doi', doi: '10.5' });
  });

  it('parses the doi: alias in any casing', () => {
    expect(parseToolbarSearch('doi:10.1')).toEqual({ kind: 'doi', doi: '10.1' });
    expect(parseToolbarSearch('DOI:10.1')).toEqual({ kind: 'doi', doi: '10.1' });
    expect(parseToolbarSearch('Doi:10.1')).toEqual({ kind: 'doi', doi: '10.1' });
  });

  // ── Journal ─────────────────────────────────────────────────────────
  it('parses j: as a journal search', () => {
    expect(parseToolbarSearch('j:nature')).toEqual({ kind: 'journal', journal: 'nature' });
  });

  it('parses J: case-insensitively', () => {
    expect(parseToolbarSearch('J:Nature')).toEqual({ kind: 'journal', journal: 'Nature' });
  });

  it('keeps the original casing of the field value', () => {
    expect(parseToolbarSearch('a:Fer')).toEqual({ kind: 'author', author: 'Fer' });
    expect(parseToolbarSearch('j:Nature')).toEqual({ kind: 'journal', journal: 'Nature' });
  });

  // ── Year ────────────────────────────────────────────────────────────
  it('parses y: as a single year (from = to)', () => {
    expect(parseToolbarSearch('y:2020')).toEqual({ kind: 'year', yearFrom: 2020, yearTo: 2020 });
  });

  it('parses the year: alias in any casing', () => {
    expect(parseToolbarSearch('year:2019')).toEqual({ kind: 'year', yearFrom: 2019, yearTo: 2019 });
    expect(parseToolbarSearch('YEAR:2019')).toEqual({ kind: 'year', yearFrom: 2019, yearTo: 2019 });
  });

  it('parses a y: from-to range', () => {
    expect(parseToolbarSearch('y:2018-2021')).toEqual({
      kind: 'year',
      yearFrom: 2018,
      yearTo: 2021,
    });
  });

  it('accepts spaces around the range dash', () => {
    expect(parseToolbarSearch('y:2018 - 2021')).toEqual({
      kind: 'year',
      yearFrom: 2018,
      yearTo: 2021,
    });
  });

  it('falls back to a plain search for non-numeric years', () => {
    expect(parseToolbarSearch('y:20x0')).toEqual({ kind: 'plain', search: 'y:20x0' });
    expect(parseToolbarSearch('y:20')).toEqual({ kind: 'plain', search: 'y:20' });
  });

  it('falls back to a plain search for out-of-bounds years', () => {
    expect(parseToolbarSearch(`y:${TOOLBAR_YEAR_MIN - 1}`)).toEqual({
      kind: 'plain',
      search: `y:${TOOLBAR_YEAR_MIN - 1}`,
    });
    expect(parseToolbarSearch(`y:${TOOLBAR_YEAR_MAX + 1}`)).toEqual({
      kind: 'plain',
      search: `y:${TOOLBAR_YEAR_MAX + 1}`,
    });
  });

  it('falls back to a plain search for a flipped range', () => {
    expect(parseToolbarSearch('y:2021-2018')).toEqual({ kind: 'plain', search: 'y:2021-2018' });
  });
});
