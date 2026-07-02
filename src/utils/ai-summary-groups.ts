/**
 * Pure helpers for organizing an AI summary's `structured_extraction` record
 * into named display groups, and for ordering `section_summaries` cards.
 *
 * The extraction prompt (`ai_article_summary_prompt.md`) has 5 field-specific
 * templates (STEM / Social Sciences / Business / Medicine / Humanities), each
 * emitting a different subset of keys. This module unions all documented keys
 * into 5 stable display groups so the view renders consistently regardless of
 * which field path the model took. Undocumented keys (forward-compat for
 * future extraction fields) are bucketed into a trailing "Other details"
 * group, which is omitted entirely when empty.
 *
 * Values may be a scalar string OR an array of strings (the prompt's templates
 * mix both shapes); the grouping logic is shape-agnostic and the view handles
 * rendering per value.
 */

import type { SectionSummary } from '@/composables/use-ai-summary';

export interface ExtractionGroup {
  name: string;
  entries: Array<[string, string | string[]]>;
}

/**
 * The 5 named display groups, in display order. Keys are matched
 * case-insensitively against the (underscore-normalized) extraction key.
 *
 * Coverage verified against `ai_article_summary_prompt.md`:
 * - STEM: research_problem, motivation, methods_models, data_sources,
 *   experiments_evaluation, key_results, contributions, limitations, future_work
 * - Social Sciences: research_questions, theoretical_framework, hypotheses,
 *   methodology, statistical_methods, key_findings, interpretation, implications
 * - Business: domain, research_question, model_theory, data_sample_period,
 *   empirical_strategy, main_results, managerial_policy_implications
 * - Medicine: clinical_area, study_type, population, intervention_exposure,
 *   comparator, outcomes, statistical_results, safety_adverse_events, conclusions
 * - Humanities: topic, thesis_argument, theoretical_lens, evidence_sources,
 *   interpretation, contribution
 *
 * Note: `motivation`, `research_question` (singular), `evidence_sources`, and
 * `contribution` (singular) are also routed below; they are covered by the
 * unions even though they appear in only one template each.
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

/**
 * Normalize an extraction key for matching: lowercase, underscores → spaces,
 * trimmed. The LLM may emit keys in either `snake_case` (per prompt) or
 * `Title Case` (some models ignore the schema), so the matcher is tolerant.
 */
function normalizeKey(key: string): string {
  return key.toLowerCase().replace(/_/g, ' ').trim();
}

/**
 * True when a value is non-empty (a non-empty string, or a non-empty array).
 * Empty strings and empty arrays are skipped so the view never renders an
 * empty `<p></p>` or an empty `<ul></ul>`.
 */
function isNonEmptyValue(value: string | string[]): boolean {
  if (Array.isArray(value)) return value.length > 0;
  return typeof value === 'string' && value.trim().length > 0;
}

/**
 * Group a `structured_extraction` record into ordered display groups.
 *
 * - Each of the 5 named groups appears (in the order above) ONLY when at least
 *   one of its keys has a non-empty value in the record.
 * - Unmatched keys with non-empty values are collected into a trailing
 *   "Other details" group, which is omitted entirely when empty.
 * - Within a group, entries preserve their first-seen order in the record
 *   (stable across runs; not HashMap-dependent).
 * - Empty values (empty string or empty array) are skipped silently.
 */
export function groupExtractionFields(
  record: Record<string, string | string[]>
): ExtractionGroup[] {
  const groups: ExtractionGroup[] = [];
  const matchedKeys = new Set<string>();

  // Pre-compute the normalized record (skip empty values once).
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

  // "Other details" bucket: any non-empty key not matched above.
  const otherEntries: Array<[string, string | string[]]> = normalizedEntries.filter(
    ([key]) => !matchedKeys.has(key)
  );
  if (otherEntries.length > 0) {
    groups.push({ name: OTHER_DETAILS_GROUP_NAME, entries: otherEntries });
  }

  return groups;
}

/**
 * Preferred display order for `section_summaries` cards. Sections present in
 * this list appear first (in this order); any other section labels follow in
 * their original (first-seen) order. This stabilizes the card layout so
 * Methods/Results/Discussion always appear in the same positions regardless
 * of HashMap iteration order or the order the model emitted them.
 */
const SECTION_PREFERRED_ORDER = ['methods', 'results', 'discussion', 'conclusion', 'introduction'];

/**
 * Sort `section_summaries` by the preferred order; unrecognized sections
 * (e.g., "Author summary", custom headings) follow in their original order.
 * Pure: returns a new array; does not mutate the input.
 */
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
