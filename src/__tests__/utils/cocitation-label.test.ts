import { describe, it, expect } from 'vitest';
import { shortPaperLabel } from '@/utils/cocitation-label';
import type { CocitationNode } from '@/types/biblio-cocitation';

/** Minimal node factory so tests don't repeat all 14 fields. */
function makeNode(overrides: Partial<CocitationNode> = {}): CocitationNode {
  return {
    id: 'rp-1',
    label: '',
    title: '',
    authors: '',
    year: null,
    journal: null,
    doi: null,
    citationCount: 0,
    coCitationCount: 0,
    matchedArticleId: null,
    matchedArticleStatus: null,
    abstract: '',
    referenceType: null,
    ...overrides,
  };
}

describe('shortPaperLabel', () => {
  it('prefers the backend-preformatted label when present', () => {
    const node = makeNode({ label: 'Rejeb et al. (2024)', authors: '["Rejeb, A."]' });
    expect(shortPaperLabel(node)).toBe('Rejeb et al. (2024)');
  });

  it('parses a JSON-array authors field and extracts the first surname (no brackets leaked)', () => {
    const node = makeNode({
      authors: '["Rejeb, A.","Saberi, B."]',
      year: 2024,
    });
    // Bug-fix assertion: previously this produced '["Rejeb ' (bracket leak).
    expect(shortPaperLabel(node)).toBe("Rejeb '24");
  });

  it('handles a single-author JSON array', () => {
    const node = makeNode({ authors: '["Smith, John"]', year: 2020 });
    expect(shortPaperLabel(node)).toBe("Smith '20");
  });

  it('handles a JSON array author without a comma (uses whole string)', () => {
    const node = makeNode({ authors: '["van der Berg"]', year: 2019 });
    expect(shortPaperLabel(node)).toBe("van der Berg '19");
  });

  it('omits the year suffix when year is null', () => {
    const node = makeNode({ authors: '["Smith, John"]' });
    expect(shortPaperLabel(node)).toBe('Smith');
  });

  it('returns Unknown when the authors array is empty', () => {
    const node = makeNode({ authors: '[]', year: 2021 });
    expect(shortPaperLabel(node)).toBe("Unknown '21");
  });

  it('returns Unknown when authors is an empty string (not valid JSON array)', () => {
    const node = makeNode({ authors: '', year: 2021 });
    expect(shortPaperLabel(node)).toBe("Unknown '21");
  });

  it('falls back to the raw string when authors is not JSON (malformed)', () => {
    const node = makeNode({ authors: 'Not JSON at all', year: 2022 });
    // The raw string is preserved as-is (no crash, no bracket leak).
    expect(shortPaperLabel(node)).toBe("Not JSON at all '22");
  });

  it('does not throw on non-string JSON array elements', () => {
    const node = makeNode({ authors: '[42, true]', year: 2023 });
    // Non-string first element -> falls through to 'Unknown'.
    expect(shortPaperLabel(node)).toBe("Unknown '23");
  });

  it('returns Unknown (no year) when both label and authors are empty', () => {
    const node = makeNode({});
    expect(shortPaperLabel(node)).toBe('Unknown');
  });
});
