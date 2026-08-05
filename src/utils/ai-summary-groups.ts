/* Pure helpers for organizing AI summary `structured_extraction` into named
 * display groups and ordering `section_summaries` cards.
 *
 * The extraction prompt has 5 field-specific templates (STEM/Social Sciences/
 * Business/Medicine/Humanities), each emitting different keys. This module
 * unions all documented keys into 5 stable groups. Undocumented keys
 * (forward-compat) go into a trailing "Other details" group (omitted when empty).
 * Values may be scalar or array; grouping is shape-agnostic. */

import type { SectionSummary } from '@/composables/use-ai-summary';

export interface ExtractionGroup {
  name: string;
  entries: Array<[string, string | string[]]>;
}

/**
 * The 5 named display groups. Keys matched case-insensitively against the
 * extraction key (underscore-normalized). Coverage verified against
 * `ai_article_summary_prompt.md` and its 5 field-specific templates.
 */
const EXTRACTION_GROUPS: Array<{ name: string; keys: string[] }> = [
  {
    name: 'Population & Context',
    keys: [
      'population',
      'clinical_area',
      'intervention_exposure',
      'comparator',
      'outcomes',
      'setting',
      'domain',
      'research_problem',
      'research_question',
      'research_questions',
      'topic',
      'motivation',
      'safety_adverse_events',
    ],
  },
  {
    name: 'Methods',
    keys: [
      'study_type',
      'methods_models',
      'methodology',
      'data_sources',
      'evidence_sources',
      'data_sample_period',
      'experiments_evaluation',
      'empirical_strategy',
      'statistical_methods',
      'theoretical_framework',
      'theoretical_lens',
      'model_theory',
    ],
  },
  {
    name: 'Results',
    keys: [
      'key_results',
      'statistical_results',
      'main_results',
      'key_findings',
      'hypotheses',
      'interpretation',
    ],
  },
  {
    name: 'Implications',
    keys: [
      'contributions',
      'contribution',
      'managerial_policy_implications',
      'implications',
      'conclusions',
    ],
  },
  {
    name: 'Limitations & Future Work',
    keys: ['limitations', 'future_work'],
  },
];

const OTHER_DETAILS_GROUP_NAME = 'Other details';

/** Normalize extraction key for matching: lowercase, underscores→spaces, trimmed. */
function normalizeKey(key: string): string {
  return key.toLowerCase().replace(/_/g, ' ').trim();
}

/** True when value is non-empty (non-empty string or non-empty array). */
function isNonEmptyValue(value: string | string[]): boolean {
  if (Array.isArray(value)) return value.length > 0;
  return typeof value === 'string' && value.trim().length > 0;
}

/**
 * Group `structured_extraction` into ordered display groups. Each named group
 * appears only when at least one key has a non-empty value. Unmatched keys
 * land in "Other details" (omitted when empty). Entries preserve first-seen
 * order. Empty values are skipped silently.
 */
export function groupExtractionFields(
  record: Record<string, string | string[]>
): ExtractionGroup[] {
  const groups: ExtractionGroup[] = [];
  const matchedKeys = new Set<string>();

  const normalizedEntries: Array<[string, string | string[]]> = Object.entries(record).filter(
    ([, value]) => isNonEmptyValue(value)
  );

  for (const groupDef of EXTRACTION_GROUPS) {
    const normalizedGroupKeys = new Set(groupDef.keys.map(normalizeKey));
    const entries: Array<[string, string | string[]]> = [];
    for (const [key, value] of normalizedEntries) {
      if (normalizedGroupKeys.has(normalizeKey(key))) {
        entries.push([key, value]);
        matchedKeys.add(key);
      }
    }
    if (entries.length > 0) {
      groups.push({ name: groupDef.name, entries });
    }
  }

  // "Other details": any non-empty key not matched above
  const otherEntries: Array<[string, string | string[]]> = normalizedEntries.filter(
    ([key]) => !matchedKeys.has(key)
  );
  if (otherEntries.length > 0) {
    groups.push({ name: OTHER_DETAILS_GROUP_NAME, entries: otherEntries });
  }

  return groups;
}

/**
 * Preferred display order for `section_summaries` cards. Sections in this
 * list appear first; others follow in original order. Stabilizes card layout
 * regardless of HashMap iteration order.
 */
const SECTION_PREFERRED_ORDER = ['methods', 'results', 'discussion', 'conclusion', 'introduction'];

/** Sort `section_summaries` by preferred order; unrecognized sections follow
 *  in original order. Pure: returns new array, does not mutate input. */
export function sortSectionSummaries(sections: SectionSummary[]): SectionSummary[] {
  const rankOf = (label: string): number => {
    const normalized = label.toLowerCase().trim();
    const idx = SECTION_PREFERRED_ORDER.findIndex((preferred) => normalized.includes(preferred));
    return idx === -1 ? Number.MAX_SAFE_INTEGER : idx;
  };
  // Stable sort by rank; ties preserve original order.
  return sections
    .map((section, originalIndex) => ({ section, originalIndex }))
    .sort((a, b) => {
      const rankA = rankOf(a.section.section);
      const rankB = rankOf(b.section.section);
      if (rankA !== rankB) return rankA - rankB;
      return a.originalIndex - b.originalIndex;
    })
    .map((entry) => entry.section);
}
