/**
 * Highlight search-term matches in rendered HTML.
 *
 * Used by the wiki page viewer to wrap occurrences of the active sidebar
 * search query in `<mark class="wiki-search-highlight">` tags so the user can
 * see where the term appears in the page body. The highlight updates live as
 * the user types and clears when the query is emptied.
 *
 * The function is HTML-aware: it splits the input into tag segments (`<...>`)
 * and text segments, leaving all markup / attributes untouched and only
 * wrapping matches inside text segments. This avoids corrupting entities,
 * breaking links, or matching inside `href`/`data-*` attributes.
 *
 * Pure + dependency-free → trivially unit-testable.
 */

/** The CSS class applied to highlighted matches. */
export const HIGHLIGHT_CLASS = 'wiki-search-highlight';

/**
 * Escape a string for safe interpolation into a `RegExp` constructor so literal
 * characters (`.`, `*`, `+`, `?`, `(`, `)`, etc.) are treated as literals.
 */
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Wrap occurrences of any search term (>= 2 chars) in `text` with
 * `<mark class="wiki-search-highlight">`. Case-insensitive. Returns the input
 * unchanged when there are no qualifying terms.
 */
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
 * {@link highlightText} to each text segment, and reassembles. Tags are passed
 * through verbatim so attributes, entities inside attributes, and nested markup
 * are never touched.
 *
 * @param html The rendered HTML string (e.g. output of `renderWikiMarkdown`).
 * @param query The raw search query (whitespace-separated terms).
 * @returns The HTML with text-segment matches wrapped in `<mark>` tags.
 *   Returns the input unchanged when the query is empty or has no qualifying terms.
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
      // Tag segment — pass through unchanged.
      out += seg;
    } else {
      // Text segment — highlight matches.
      out += highlightText(seg, terms);
    }
  }
  return out;
}
