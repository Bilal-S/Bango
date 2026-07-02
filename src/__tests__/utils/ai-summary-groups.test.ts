import { describe, it, expect } from 'vitest';
import { groupExtractionFields, sortSectionSummaries } from '@/utils/ai-summary-groups';
import type { SectionSummary } from '@/composables/use-ai-summary';

describe('groupExtractionFields', () => {
  it('routes known keys to their named groups in display order', () => {
    const groups = groupExtractionFields({
      // Population & Context
      population: 'Children aged 5-17 in England',
      // Methods
      study_type: 'Controlled interrupted time series',
      // Results
      key_results: ['15g/week reduction', '19,500 QALYs'],
      // Implications
      conclusions: 'SDIL is predicted to improve child health',
      // Limitations & Future Work
      limitations: ['Home-only data', 'Counterfactual assumption'],
    });

    const names = groups.map((g) => g.name);
    expect(names).toEqual([
      'Population & Context',
      'Methods',
      'Results',
      'Implications',
      'Limitations & Future Work',
    ]);
  });

  it('preserves array values as arrays (no string coercion)', () => {
    const groups = groupExtractionFields({
      key_results: ['15g/week reduction', '19,500 QALYs'],
      population: 'Children aged 5-17',
    });
    const resultsGroup = groups.find((g) => g.name === 'Results');
    expect(resultsGroup).toBeDefined();
    expect(resultsGroup!.entries.length).toBeGreaterThan(0);
    const entry = resultsGroup!.entries[0]!;
    expect(entry[0]).toBe('key_results');
    expect(Array.isArray(entry[1])).toBe(true);
    expect(entry[1]).toEqual(['15g/week reduction', '19,500 QALYs']);
  });

  it('preserves scalar values as strings', () => {
    const groups = groupExtractionFields({ population: 'Children aged 5-17' });
    const popGroup = groups.find((g) => g.name === 'Population & Context');
    expect(popGroup).toBeDefined();
    expect(popGroup!.entries.length).toBeGreaterThan(0);
    const entry = popGroup!.entries[0]!;
    expect(typeof entry[1]).toBe('string');
    expect(entry[1]).toBe('Children aged 5-17');
  });

  it('buckets unmatched keys into "Other details"', () => {
    const groups = groupExtractionFields({
      population: 'Children',
      new_field_the_llm_invented: ['novel fact 1', 'novel fact 2'],
    });
    const other = groups.find((g) => g.name === 'Other details');
    expect(other).toBeDefined();
    expect(other!.entries).toHaveLength(1);
    const otherEntry = other!.entries[0]!;
    expect(otherEntry[0]).toBe('new_field_the_llm_invented');
  });

  it('omits the "Other details" group entirely when all keys are known', () => {
    const groups = groupExtractionFields({ population: 'Children', study_type: 'RCT' });
    expect(groups.find((g) => g.name === 'Other details')).toBeUndefined();
  });

  it('skips empty values (empty string and empty array)', () => {
    const groups = groupExtractionFields({
      population: 'Children',
      empty_string_key: '',
      empty_array_key: [] as string[],
    });
    // Only the non-empty value should appear anywhere.
    const allKeys = groups.flatMap((g) => g.entries.map(([k]) => k));
    expect(allKeys).toEqual(['population']);
    expect(allKeys).not.toContain('empty_string_key');
    expect(allKeys).not.toContain('empty_array_key');
  });

  it('omits a named group entirely when none of its keys are present', () => {
    const groups = groupExtractionFields({
      population: 'Children',
      // No Methods / Results / Implications / Limitations keys.
    });
    const names = groups.map((g) => g.name);
    expect(names).toEqual(['Population & Context']);
  });

  it('returns an empty array for an empty record', () => {
    expect(groupExtractionFields({})).toEqual([]);
  });

  it('matches keys case-insensitively and tolerates Title Case', () => {
    // Some models emit "Study Type" instead of "study_type".
    const groups = groupExtractionFields({
      'Study Type': 'RCT',
      POPULATION: 'Adults',
    });
    expect(groups.find((g) => g.name === 'Methods')?.entries).toHaveLength(1);
    expect(groups.find((g) => g.name === 'Population & Context')?.entries).toHaveLength(1);
  });

  it('preserves first-seen order within a group (stable, not HashMap-order)', () => {
    // JS object key order is insertion-order for string keys, so construct with
    // outcomes before population (both in Population & Context) and verify.
    const groups = groupExtractionFields({
      outcomes: ['BMI', 'QALYs'],
      population: 'Children',
    });
    const popGroup = groups.find((g) => g.name === 'Population & Context');
    expect(popGroup!.entries.map(([k]) => k)).toEqual(['outcomes', 'population']);
  });
});

describe('sortSectionSummaries', () => {
  it('orders Methods, Results, Discussion first (regardless of input order)', () => {
    const input: SectionSummary[] = [
      { section: 'Discussion', summary: 'd' },
      { section: 'Methods', summary: 'm' },
      { section: 'Results', summary: 'r' },
    ];
    const sorted = sortSectionSummaries(input);
    expect(sorted.map((s) => s.section)).toEqual(['Methods', 'Results', 'Discussion']);
  });

  it('places Conclusion and Introduction after the core three', () => {
    const input: SectionSummary[] = [
      { section: 'Introduction', summary: 'i' },
      { section: 'Conclusion', summary: 'c' },
      { section: 'Methods', summary: 'm' },
    ];
    const sorted = sortSectionSummaries(input);
    expect(sorted.map((s) => s.section)).toEqual(['Methods', 'Conclusion', 'Introduction']);
  });

  it('preserves original order for unrecognized sections (after recognized ones)', () => {
    const input: SectionSummary[] = [
      { section: 'Author Summary', summary: 'a' },
      { section: 'Methods', summary: 'm' },
      { section: 'Custom Heading', summary: 'x' },
    ];
    const sorted = sortSectionSummaries(input);
    // Methods first; the two unrecognized follow in original order.
    expect(sorted.map((s) => s.section)).toEqual(['Methods', 'Author Summary', 'Custom Heading']);
  });

  it('does not mutate the input array', () => {
    const input: SectionSummary[] = [
      { section: 'Results', summary: 'r' },
      { section: 'Methods', summary: 'm' },
    ];
    const snapshot = input.map((s) => s.section);
    sortSectionSummaries(input);
    expect(input.map((s) => s.section)).toEqual(snapshot);
  });

  it('handles an empty array', () => {
    expect(sortSectionSummaries([])).toEqual([]);
  });
});
