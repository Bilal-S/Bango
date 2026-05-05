import { describe, it, expect } from 'vitest';
import { formatDate, formatConfidence, formatPriority } from '@/utils/formatters';

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
    expect(formatConfidence(null)).toBe('—');
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
