/**
 * Toolbar search field prefixes, longest-first so `doi:`/`year:` are never
 * read as `d:`/`y:`. Matching is case-insensitive.
 */
const FIELD_PREFIXES = ['doi:', 'year:', 'a:', 'd:', 'j:', 'y:'] as const;

/** Year bounds mirroring the filter panel's Year validation. */
export const TOOLBAR_YEAR_MIN = 1850;
export const TOOLBAR_YEAR_MAX = 2100;

/** Single 4-digit year or `from - to` range (spaces around the dash allowed). */
const YEAR_VALUE_RE = /^(?<from>\d{4})\s*(?:-\s*(?<to>\d{4}))?$/;

/** Parsed toolbar search: one target filter field + its value, or free text. */
export type ToolbarSearch =
  | { kind: 'plain'; search: string }
  | { kind: 'author'; author: string }
  | { kind: 'doi'; doi: string }
  | { kind: 'journal'; journal: string }
  | { kind: 'year'; yearFrom: number; yearTo: number };

/** The field-targeted variants of {@link ToolbarSearch} (everything but plain). */
export type ToolbarFieldSearch = Extract<
  ToolbarSearch,
  { kind: 'author' | 'doi' | 'journal' | 'year' }
>;

/**
 * Parse a toolbar search into its target filter field. A recognized prefix
 * (`a:`, `d:`/`doi:`, `j:`, `y:`/`year:`, case-insensitive, start-of-text only)
 * routes everything after it to that panel field; anything else (bare prefix,
 * invalid year, or a prefix mid-string) stays a plain title/abstract/notes
 * search so nothing is silently dropped.
 * @param text - Raw toolbar search box content.
 * @returns The parsed search (field kind + value, or plain text).
 */
export function parseToolbarSearch(text: string): ToolbarSearch {
  const trimmed = text.trim();
  const lower = trimmed.toLowerCase();
  const prefix = FIELD_PREFIXES.find((p) => lower.startsWith(p));
  if (prefix === undefined) return { kind: 'plain', search: trimmed };

  const value = trimmed.slice(prefix.length).trim();
  if (!value) return { kind: 'plain', search: '' };

  switch (prefix) {
    case 'a:':
      return { kind: 'author', author: value };
    case 'd:':
    case 'doi:':
      return { kind: 'doi', doi: value };
    case 'j:':
      return { kind: 'journal', journal: value };
    case 'y:':
    case 'year:':
      return parseYearValue(value, trimmed);
    default:
      return { kind: 'plain', search: trimmed };
  }
}

/**
 * Validate + split a `y:` value. Falls back to the plain search when the value
 * is not one or two 4-digit years, lies outside the panel's bounds, or flips
 * the range (the panel disables Apply for the same inputs).
 * @param value - Text after the `y:`/`year:` prefix.
 * @param plain - Original full text for the fallback.
 * @returns A year search or the plain fallback.
 */
function parseYearValue(value: string, plain: string): ToolbarSearch {
  const groups = YEAR_VALUE_RE.exec(value)?.groups;
  if (groups) {
    const fromStr = groups.from;
    const toStr = groups.to;
    if (fromStr !== undefined) {
      const from = Number(fromStr);
      const to = toStr !== undefined ? Number(toStr) : from;
      const inBounds = (y: number): boolean => y >= TOOLBAR_YEAR_MIN && y <= TOOLBAR_YEAR_MAX;
      if (inBounds(from) && inBounds(to) && from <= to) {
        return { kind: 'year', yearFrom: from, yearTo: to };
      }
    }
  }
  return { kind: 'plain', search: plain };
}
