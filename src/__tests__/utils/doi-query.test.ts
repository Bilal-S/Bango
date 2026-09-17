import { describe, it, expect } from 'vitest';

/* Pure mirror of the Rust `extract_doi_query` gate (ris::doi::normalize_doi +
 * is_bare_doi): pins the accepted input forms and the rejections so the
 * cosmetic search hint can never disagree with the backend URL decision. */

import { isDoiQuery, normalizeDoiQuery } from '@/utils/doi-query';

describe('doi-query', () => {
  it('accepts bare, url, scheme, and whitespace-padded forms and canonicalizes', () => {
    expect(normalizeDoiQuery('10.1016/j.puhe.2018.04.012')).toBe('10.1016/j.puhe.2018.04.012');
    expect(normalizeDoiQuery('https://doi.org/10.1/AbC')).toBe('10.1/abc');
    expect(normalizeDoiQuery('http://dx.doi.org/10.1/X')).toBe('10.1/x');
    expect(normalizeDoiQuery('doi:10.1/x')).toBe('10.1/x');
    expect(normalizeDoiQuery('  10.1/Spaced  ')).toBe('10.1/spaced');
  });

  it('rejects plain queries, placeholders, and doi-shaped non-dois', () => {
    expect(normalizeDoiQuery('sugar tax')).toBeNull();
    expect(normalizeDoiQuery('10.1016 and health')).toBeNull();
    expect(normalizeDoiQuery('NA')).toBeNull();
    expect(normalizeDoiQuery('')).toBeNull();
    expect(isDoiQuery('sugar tax')).toBe(false);
    expect(isDoiQuery('10.1/abc')).toBe(true);
  });
});
