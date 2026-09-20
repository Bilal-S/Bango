/* Pure helpers for co-citation paper label formatting.
 * Extracted from `cocitation-heatmap.vue` for unit-testability. */
import type { CocitationNode } from '@/types/biblio-cocitation';
import { parseAuthorList } from './formatters';

/** Build short axis label for a co-citation paper. Prefers `node.label`, falls
 *  back to parsing JSON-array `authors` field (never leaks array brackets). */
export function shortPaperLabel(node: CocitationNode): string {
  // 1. Backend-preformatted label is the source of truth (matches the graph nodes).
  if (node.label) return node.label;

  // 2. Fallback: parse the JSON-array authors field safely via the shared
  // parser. An empty result (blank, `[]`, or all-non-string elements) maps to
  // Unknown; non-JSON strings come back as their delimited segments so a
  // single malformed name still surfaces (minus surrounding whitespace).
  let lastName = 'Unknown';
  const authors = parseAuthorList(node.authors);
  if (authors.length > 0) {
    const first = authors[0]!;
    // Handle "Last, First" -> "Last"; otherwise use the whole string.
    lastName = first.split(',')[0]?.trim() || first;
  }

  const yearSuffix = node.year ? ` '${String(node.year).slice(-2)}` : '';
  return `${lastName}${yearSuffix}`;
}
