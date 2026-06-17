import { describe, it, expect } from 'vitest';
import {
  formatDate,
  formatConfidence,
  formatPriority,
  formatArticleCount,
  stripUuidFromDetails,
  getFullTextFileIcon,
  formatAuthors,
  doiLink,
  getPublicationTypeLabel,
} from '@/utils/formatters';

describe('formatters (extended)', () => {
  describe('formatDate', () => {
    it('formats an ISO date string with year and month', () => {
      // Use noon UTC to avoid off-by-one timezone shifts in local rendering.
      const result = formatDate('2026-01-15T12:00:00Z');
      expect(result).toContain('2026');
      expect(result).toMatch(/Jan/);
    });
  });

  describe('formatConfidence', () => {
    it('returns dash for null', () => {
      expect(formatConfidence(null)).toBe('-');
    });
    it('rounds to percentage', () => {
      expect(formatConfidence(0.856)).toBe('86%');
      expect(formatConfidence(0.5)).toBe('50%');
      expect(formatConfidence(0)).toBe('0%');
      expect(formatConfidence(1)).toBe('100%');
    });
  });

  describe('formatPriority', () => {
    it('capitalizes the first letter', () => {
      expect(formatPriority('critical')).toBe('Critical');
      expect(formatPriority('high')).toBe('High');
    });
    it('leaves already-capitalized strings mostly intact', () => {
      expect(formatPriority('Standard')).toBe('Standard');
    });
  });

  describe('formatArticleCount', () => {
    it('singular for 1', () => {
      expect(formatArticleCount(1)).toBe('1 article');
    });
    it('plural for 0 and >1', () => {
      expect(formatArticleCount(0)).toBe('0 articles');
      expect(formatArticleCount(5)).toBe('5 articles');
    });
  });

  describe('stripUuidFromDetails', () => {
    it('returns null for null input', () => {
      expect(stripUuidFromDetails(null)).toBeNull();
    });
    it('returns null for empty string', () => {
      expect(stripUuidFromDetails('')).toBeNull();
    });
    it('strips a UUID', () => {
      const input = 'Status changed of article 550e8400-e29b-41d4-a716-446655440000';
      const result = stripUuidFromDetails(input);
      expect(result).not.toContain('550e8400');
      expect(result).toContain('Status changed');
    });
    it('removes dangling "of article" preposition after UUID strip', () => {
      const input = 'Deleted of article 550e8400-e29b-41d4-a716-446655440000';
      const result = stripUuidFromDetails(input)!;
      expect(result).not.toContain('of article');
      expect(result.toLowerCase()).toContain('deleted');
    });
    it('removes dangling "into article" preposition', () => {
      const input = 'Merged into article 550e8400-e29b-41d4-a716-446655440000';
      const result = stripUuidFromDetails(input)!;
      expect(result.toLowerCase()).not.toContain('into article');
    });
    it('collapses multiple spaces', () => {
      const input = 'Text   with    gaps 550e8400-e29b-41d4-a716-446655440000';
      const result = stripUuidFromDetails(input)!;
      expect(result).not.toMatch(/\s{2,}/);
    });
  });

  describe('getFullTextFileIcon', () => {
    it('returns null for null/undefined/empty', () => {
      expect(getFullTextFileIcon(null)).toBeNull();
      expect(getFullTextFileIcon(undefined)).toBeNull();
      expect(getFullTextFileIcon('')).toBeNull();
    });
    it('returns pdf icon for .pdf', () => {
      expect(getFullTextFileIcon('paper.pdf')).toBe('picture_as_pdf');
      expect(getFullTextFileIcon('PAPER.PDF')).toBe('picture_as_pdf');
    });
    it('returns description icon for .txt', () => {
      expect(getFullTextFileIcon('notes.txt')).toBe('description');
    });
    it('returns draft icon for other extensions', () => {
      expect(getFullTextFileIcon('file.docx')).toBe('draft');
      expect(getFullTextFileIcon('file')).toBe('draft');
    });
  });

  describe('formatAuthors', () => {
    it('returns empty for null/undefined/empty', () => {
      expect(formatAuthors(null)).toBe('');
      expect(formatAuthors(undefined)).toBe('');
      expect(formatAuthors([])).toBe('');
    });
    it('returns joined authors when count <= limit', () => {
      expect(formatAuthors(['Smith, J.', 'Doe, A.'])).toBe('Smith, J., Doe, A.');
      expect(formatAuthors(['Only One'])).toBe('Only One');
    });
    it('uses custom separator', () => {
      expect(formatAuthors(['A', 'B'], 3, ' & ')).toBe('A & B');
    });
    it('returns "First et al." when count exceeds limit', () => {
      expect(formatAuthors(['A', 'B', 'C', 'D'], 3)).toBe('A et al.');
    });
    it('respects custom limit', () => {
      expect(formatAuthors(['A', 'B', 'C'], 2)).toBe('A et al.');
    });
  });

  describe('doiLink', () => {
    it('returns undefined for null/undefined', () => {
      expect(doiLink(null)).toBeUndefined();
      expect(doiLink(undefined)).toBeUndefined();
    });
    it('returns the doi.org URL for a bare DOI', () => {
      expect(doiLink('10.1000/foo')).toBe('https://doi.org/10.1000/foo');
    });
    it('returns the input unchanged when it already starts with http', () => {
      expect(doiLink('https://doi.org/10.1000/foo')).toBe('https://doi.org/10.1000/foo');
      expect(doiLink('http://example.com')).toBe('http://example.com');
    });
  });

  describe('getPublicationTypeLabel', () => {
    it('returns "Publication" for null/undefined', () => {
      expect(getPublicationTypeLabel(null)).toBe('Publication');
      expect(getPublicationTypeLabel(undefined)).toBe('Publication');
    });
    it('maps known codes case-insensitively', () => {
      expect(getPublicationTypeLabel('JOUR')).toBe('Journal');
      expect(getPublicationTypeLabel('jour')).toBe('Journal');
      expect(getPublicationTypeLabel('BOOK')).toBe('Book');
      expect(getPublicationTypeLabel('CHAP')).toBe('Chapter');
      expect(getPublicationTypeLabel('CONF')).toBe('Conference');
      expect(getPublicationTypeLabel('RPRT')).toBe('Reports');
      expect(getPublicationTypeLabel('THES')).toBe('Theses');
      expect(getPublicationTypeLabel('PAT')).toBe('Patent');
    });
    it('returns "Publication" for unknown codes', () => {
      expect(getPublicationTypeLabel('UNKNOWN')).toBe('Publication');
    });
    it('trims whitespace before lookup', () => {
      expect(getPublicationTypeLabel('  JOUR  ')).toBe('Journal');
    });
  });
});
