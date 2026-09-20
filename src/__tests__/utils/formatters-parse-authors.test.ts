import { describe, it, expect } from 'vitest';
import { formatAuthors, parseAuthorList } from '@/utils/formatters';

describe('parseAuthorList', () => {
  it('parses_a_json_encoded_author_array', () => {
    const raw = JSON.stringify(['Pell, D', 'Mytton, O', 'Penney, TL']);
    expect(parseAuthorList(raw)).toEqual(['Pell, D', 'Mytton, O', 'Penney, TL']);
  });

  it('drops_non_string_and_empty_array_elements', () => {
    expect(parseAuthorList('[42, true, "Smith, J", ""]')).toEqual(['Smith, J']);
  });

  it('splits_a_plain_delimited_string_on_semicolons', () => {
    expect(parseAuthorList('Smith J, Doe A; Roe, R')).toEqual(['Smith J, Doe A', 'Roe, R']);
  });

  it('keeps_a_plain_single_name_string_as_one_entry', () => {
    expect(parseAuthorList('Smith J, Doe A')).toEqual(['Smith J, Doe A']);
  });

  it('returns_empty_for_null_empty_and_blank_inputs', () => {
    expect(parseAuthorList(null)).toEqual([]);
    expect(parseAuthorList(undefined)).toEqual([]);
    expect(parseAuthorList('')).toEqual([]);
    expect(parseAuthorList('[]')).toEqual([]);
    expect(parseAuthorList('   ')).toEqual([]);
  });

  it('feeds_formatAuthors_without_leaking_array_syntax', () => {
    const raw = JSON.stringify(['Pell, D', 'Mytton, O', 'Penney, TL']);
    const list = parseAuthorList(raw);
    const rendered = formatAuthors(list, list.length);
    expect(rendered).toBe('Pell, D, Mytton, O, Penney, TL');
    expect(rendered).not.toContain('[');
  });
});
