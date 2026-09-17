/**
 * Pure DOI-direct detection for the OpenAlex search box.
 * Mirrors the Rust `ris::doi::normalize_doi` + `is_bare_doi` pair so the
 * cosmetic hint can never disagree with the backend's URL decision.
 */

const DOI_PREFIXES = [
  'https://doi.org/',
  'http://doi.org/',
  'https://dx.doi.org/',
  'http://dx.doi.org/',
  'doi:',
];
const DOI_PLACEHOLDERS = ['na', 'n/a', 'null', 'none', '-'];

function stripPrefixCi(text: string): string {
  for (const prefix of DOI_PREFIXES) {
    if (text.toLowerCase().startsWith(prefix)) return text.slice(prefix.length);
  }
  return text;
}

/**
 * Canonicalize free text into a bare DOI (`10.x/y`, lowercase).
 * @param text raw search-box text
 * @returns the canonical DOI, or null when the text is not a DOI (plain query, placeholder, empty)
 */
export function normalizeDoiQuery(text: string): string | null {
  const stripped = stripPrefixCi(text.trim()).trim().toLowerCase();
  if (!stripped) return null;
  if (DOI_PLACEHOLDERS.includes(stripped)) return null;
  if (!(stripped.startsWith('10.') && stripped.includes('/'))) return null;
  return stripped;
}

/**
 * True when the search-box text is a DOI (bare, `doi:` scheme, or doi.org URL form).
 * @param text raw search-box text
 */
export function isDoiQuery(text: string): boolean {
  return normalizeDoiQuery(text) !== null;
}
