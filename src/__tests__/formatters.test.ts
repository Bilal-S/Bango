import { describe, it, expect } from 'vitest';
import {
  formatDate,
  formatConfidence,
  formatPriority,
  formatArticleCount,
  folderLabelFromPath,
  getPublicationTypeLabel,
} from '@/utils/formatters';

describe('formatDate', () => {
  it('formats ISO string to locale date', () => {
    const result = formatDate('2023-06-15T10:30:00Z');
    expect(result).toContain('2023');
    expect(result).toContain('Jun');
  });
});

describe('formatConfidence', () => {
  it('formats confidence as percentage', () => {
    expect(formatConfidence(0.856)).toBe('86%');
  });

  it('returns dash for null', () => {
    expect(formatConfidence(null)).toBe('-');
  });

  it('rounds correctly', () => {
    expect(formatConfidence(0.999)).toBe('100%');
  });
});

describe('formatPriority', () => {
  it('capitalizes first letter', () => {
    expect(formatPriority('critical')).toBe('Critical');
    expect(formatPriority('standard')).toBe('Standard');
  });
});

describe('formatArticleCount', () => {
  it('uses plural for zero', () => {
    expect(formatArticleCount(0)).toBe('0 articles');
  });

  it('uses singular for one', () => {
    expect(formatArticleCount(1)).toBe('1 article');
  });

  it('uses plural for more than one', () => {
    expect(formatArticleCount(2)).toBe('2 articles');
    expect(formatArticleCount(142)).toBe('142 articles');
  });
});

describe('getPublicationTypeLabel', () => {
  it('maps known tags to friendly names', () => {
    expect(getPublicationTypeLabel('JOUR')).toBe('Journal');
    expect(getPublicationTypeLabel('BOOK')).toBe('Book');
    expect(getPublicationTypeLabel('CHAP')).toBe('Chapter');
    expect(getPublicationTypeLabel('CONF')).toBe('Conference');
    expect(getPublicationTypeLabel('RPRT')).toBe('Reports');
    expect(getPublicationTypeLabel('MAGZ')).toBe('Magazine');
    expect(getPublicationTypeLabel('NEWS')).toBe('Newspaper');
    expect(getPublicationTypeLabel('THES')).toBe('Theses');
    expect(getPublicationTypeLabel('ELEC')).toBe('Electronic/Web');
    expect(getPublicationTypeLabel('DATA')).toBe('Data files');
    expect(getPublicationTypeLabel('ART')).toBe('Artwork');
    expect(getPublicationTypeLabel('BILL')).toBe('Bills');
    expect(getPublicationTypeLabel('PAMP')).toBe('Pamphlet');
    expect(getPublicationTypeLabel('PAT')).toBe('Patent');
    expect(getPublicationTypeLabel('VIDEO')).toBe('Video');
    expect(getPublicationTypeLabel('SOUND')).toBe('Sound');
  });

  it('is case-insensitive and trims whitespace', () => {
    expect(getPublicationTypeLabel(' jour ')).toBe('Journal');
    expect(getPublicationTypeLabel('Book')).toBe('Book');
  });

  it('falls back to Publication for unknown tags or null/undefined', () => {
    expect(getPublicationTypeLabel(null)).toBe('Publication');
    expect(getPublicationTypeLabel(undefined)).toBe('Publication');
    expect(getPublicationTypeLabel('UNKNOWN')).toBe('Publication');
  });
});

describe('folderLabelFromPath', () => {
  it('returns the last segment with a trailing slash', () => {
    expect(folderLabelFromPath('/home/user/Documents/Bango')).toBe('Bango/');
  });

  it('strips trailing separators before taking the last segment', () => {
    expect(folderLabelFromPath('/home/user/Documents/Bango/')).toBe('Bango/');
    expect(folderLabelFromPath('/data/my-research/')).toBe('my-research/');
  });

  it('normalizes Windows backslashes', () => {
    expect(folderLabelFromPath('D:\\Research\\Bango Project')).toBe('Bango Project/');
  });

  it('handles a custom directory name', () => {
    expect(folderLabelFromPath('/data/my-research')).toBe('my-research/');
  });

  it('falls back to Bango/ for empty or root-only paths', () => {
    expect(folderLabelFromPath('')).toBe('Bango/');
    expect(folderLabelFromPath('/')).toBe('Bango/');
    expect(folderLabelFromPath('///')).toBe('Bango/');
  });

  it('preserves spaces in folder names', () => {
    expect(folderLabelFromPath('/home/user/My Bango Data')).toBe('My Bango Data/');
  });
});
