/* Highlight search-term matches in rendered HTML.
 * HTML-aware: splits into tag (`<...>`) and text segments, leaves markup
 * untouched, wraps matches in text segments with `<mark class="wiki-search-highlight">`.
 * Pure + dependency-free → unit-testable. */

/** The CSS class applied to highlighted matches. */
export const HIGHLIGHT_CLASS = 'wiki-search-highlight';

/** Escape literal chars for safe RegExp interpolation. */
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Wrap occurrences of search terms (>= 2 chars) in text with `<mark>`. Case-insensitive. */
function highlightText(text: string, terms: string[]): string {
  if (terms.length === 0) return text;
  // Build one alternation pattern: (term1|term2|...). Each term is escaped.
  const pattern = new RegExp(`(${terms.map(escapeRegExp).join('|')})`, 'gi');
  return text.replace(pattern, (match) => `<mark class="${HIGHLIGHT_CLASS}">${match}</mark>`);
}

/**
 * Extract the qualifying search terms (>= 2 alphanumeric characters) from a
 * raw query string. Shorter tokens are dropped to avoid highlighting every
 * occurrence of common single letters / punctuation.
 */
export function extractSearchTerms(query: string): string[] {
  const trimmed = query.trim();
  if (!trimmed) return [];
  return trimmed
    .split(/\s+/)
    .filter((t) => t.length >= 2 && /[a-z0-9]/i.test(t))
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
}

/**
 * Highlight search-term matches in rendered HTML.
 *
 * Splits `html` into alternating tag (`<...>`) and text segments, applies
 * `highlightText` to each text segment, reassembles verbatim. Tags pass
 * through untouched.
 *
 * @returns HTML with text-segment matches wrapped in `<mark>` tags, or input
 *   unchanged when query is empty or has no qualifying terms.
 */
export function highlightSearchTerms(html: string, query: string): string {
  const terms = extractSearchTerms(query);
  if (terms.length === 0) return html;

  // Split into tag and non-tag segments. The capturing group keeps the
  // delimiters in the result array so we can classify each piece.
  const segments = html.split(/(<[^>]*>)/g);
  let out = '';
  for (const seg of segments) {
    if (seg.startsWith('<')) {
      // Tag segment - pass through unchanged.
      out += seg;
    } else {
      // Text segment - highlight matches.
      out += highlightText(seg, terms);
    }
  }
  return out;
}
