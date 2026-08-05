/* Pure helpers for co-citation paper label formatting.
 * Extracted from `cocitation-heatmap.vue` for unit-testability. */
import type { CocitationNode } from '@/types/biblio-cocitation';

/** Build short axis label for a co-citation paper. Prefers `node.label`, falls
 *  back to parsing JSON-array `authors` field (never leaks array brackets). */
export function shortPaperLabel(node: CocitationNode): string {
  // 1. Backend-preformatted label is the source of truth (matches the graph nodes).
  if (node.label) return node.label;

  // 2. Fallback: parse the JSON-array authors field safely.
  let lastName = 'Unknown';
  try {
    const arr = JSON.parse(node.authors || '[]') as unknown;
    if (Array.isArray(arr) && arr.length > 0 && typeof arr[0] === 'string') {
      const first = arr[0] as string;
      // Handle "Last, First" -> "Last"; otherwise use the whole string.
      lastName = first.split(',')[0]?.trim() || first;
    }
  } catch {
    // Not JSON; fall back to the raw string trimmed.
    lastName = (node.authors || 'Unknown').trim();
  }

  const yearSuffix = node.year ? ` '${String(node.year).slice(-2)}` : '';
  return `${lastName}${yearSuffix}`;
}
