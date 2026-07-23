/**
 * Pure helpers for co-citation paper label formatting.
 *
 * Extracted from `cocitation-heatmap.vue` so the JSON-array parsing logic is
 * unit-testable without mounting the ApexCharts heatmap component.
 */
import type { CocitationNode } from '@/types/biblio-cocitation';

/**
 * Build a short axis/category label for a co-citation paper.
 *
 * Prefers the backend-preformatted `node.label` (already JSON-aware via the
 * Rust `format_paper_label`), and falls back to parsing the JSON-array
 * `authors` field safely so the axis never leaks array brackets like
 * `["Rejeb ` (the bug that occurred when assuming `;`-delimited authors).
 *
 * @param node - The co-citation node to label.
 * @returns The short label string (e.g. `"Rejeb et al. (2024)"`).
 */
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
