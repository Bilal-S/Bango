export function formatDate(isoString: string): string {
  return new Date(isoString).toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export function formatConfidence(confidence: number | null): string {
  if (confidence === null) return '-';
  return `${Math.round(confidence * 100)}%`;
}

export function formatPriority(priority: string): string {
  return priority.charAt(0).toUpperCase() + priority.slice(1);
}

export function formatArticleCount(count: number): string {
  return `${count} article${count === 1 ? '' : 's'}`;
}

/**
 * Strip any UUID (8-4-4-4-12 hex pattern) from a details string.
 * Also cleans up dangling prepositions like "of article " or "into article " left behind.
 */
export function stripUuidFromDetails(details: string | null): string | null {
  if (!details) return null;
  const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
  let result = details.replace(UUID_RE, '').trim();
  // Clean up trailing dangling prepositions: "of article", "into article"
  result = result.replace(/\s+(of|into)\s+article\s*$/i, '').trim();
  // Collapse multiple spaces left behind
  result = result.replace(/\s{2,}/g, ' ').trim();
  return result || null;
}

/**
 * Determine the file type icon based on filename.
 */
export function getFullTextFileIcon(fileName: string | null | undefined): string | null {
  if (!fileName) return null;
  const lower = fileName.toLowerCase();
  if (lower.endsWith('.pdf')) return 'picture_as_pdf';
  if (lower.endsWith('.txt')) return 'description';
  return 'draft';
}

/**
 * Format a list of author names.
 */
export function formatAuthors(
  authors: string[] | null | undefined,
  limit = 3,
  separator = ', '
): string {
  if (!authors || !authors.length) return '';
  if (authors.length <= limit) return authors.join(separator);
  return `${authors[0]} et al.`;
}

/**
 * Create a DOI hyperlink if possible.
 */
export function doiLink(doi: string | null | undefined): string | undefined {
  if (!doi) return undefined;
  return doi.startsWith('http') ? doi : `https://doi.org/${doi}`;
}

/**
 * Average occurrences per year across the active span of a term.
 *
 * Computed as total occurrences divided by the inclusive year span
 * (lastYear - firstYear + 1). Returns `null` when there is no year data.
 *
 * Examples:
 * - 3 occurrences across 2018, 2020, 2024 -> 3 / 7 ≈ 0.43
 * - 10 occurrences all in 2020             -> 10 / 1 = 10.0
 */
export function avgPerYear(
  yearCounts: { year: number; count: number }[] | null | undefined
): number | null {
  if (!yearCounts || yearCounts.length === 0) return null;
  let minYear = Infinity;
  let maxYear = -Infinity;
  let total = 0;
  for (const yc of yearCounts) {
    if (yc.year < minYear) minYear = yc.year;
    if (yc.year > maxYear) maxYear = yc.year;
    total += yc.count;
  }
  const span = maxYear - minYear + 1;
  if (span <= 0) return null;
  return total / span;
}

/**
 * Convert a short publication type code (e.g. JOUR, BOOK) to a human-friendly label.
 * Defaults to 'Publication' if clean code is not recognized or not provided.
 */
export function getPublicationTypeLabel(type: string | null | undefined): string {
  if (!type) return 'Publication';
  const cleanType = type.trim().toUpperCase();
  const map: Record<string, string> = {
    JOUR: 'Journal',
    BOOK: 'Book',
    CHAP: 'Chapter',
    CONF: 'Conference',
    RPRT: 'Reports',
    MAGZ: 'Magazine',
    NEWS: 'Newspaper',
    THES: 'Theses',
    ELEC: 'Electronic/Web',
    DATA: 'Data files',
    ART: 'Artwork',
    BILL: 'Bills',
    PAMP: 'Pamphlet',
    PAT: 'Patent',
    VIDEO: 'Video',
    SOUND: 'Sound',
  };
  return map[cleanType] || 'Publication';
}

/**
 * Derive the display label for the storage-root folder in the Settings
 * directory tree. Returns the last path segment plus a trailing slash so the
 * tree root reads naturally (e.g. `/data/my-research` -> `my-research/`).
 *
 * Trailing separators are stripped before taking the last segment so
 * `/home/u/Documents/Bango/` still resolves to `Bango/`. Falls back to
 * `Bango/` when the path is empty, root-only (`/`), or has no discernible
 * segment so the tree never shows a blank root.
 *
 * @param path - the effective storage root path (forward or back slashes).
 * @returns the last segment with a trailing slash.
 */
export function folderLabelFromPath(path: string): string {
  // Normalize backslashes (Windows) to forward slashes for uniform splitting,
  // then drop trailing separators so the last split is the real folder name.
  const normalized = path.replace(/\\/g, '/').replace(/\/+$/, '');
  const segments = normalized.split('/').filter((s) => s.length > 0);
  const last = segments[segments.length - 1];
  return last ? `${last}/` : 'Bango/';
}

/**
 * Truncate `text` to at most `maxLen` characters at the last word boundary
 * (never mid-word), appending an ellipsis (`...`) when truncation occurs.
 *
 * - If `text` already fits, it is returned verbatim (no trimming, no ellipsis).
 * - If the first word alone exceeds `maxLen`, it is hard-truncated at `maxLen`
 *   with an ellipsis (so a long single token still yields a sensible label).
 * - Surrounding whitespace is trimmed before measuring, and any whitespace
 *   preceding the cut point is removed so the ellipsis sits flush against the
 *   last kept word.
 *
 * Counts characters by `Array.from(text)` code points (not UTF-16 code units),
 * so multi-byte text (CJK, emoji) is counted the same way a browser counts
 * `maxlength`.
 *
 * @param text - the source string (e.g. a research aim).
 * @param maxLen - the maximum character length of the result, excluding the
 *   ellipsis. Must be >= 1.
 * @returns the truncated string (with `...` when shortened), or the trimmed
 *   original when it already fits.
 * @example truncateAtWordBoundary('A short aim', 20) // 'A short aim'
 * @example truncateAtWordBoundary('A very long research aim about obesity', 20) // 'A very long...'
 * @example truncateAtWordBoundary('supercalifragilistic', 10) // 'supercalif...'
 */
export function truncateAtWordBoundary(text: string, maxLen: number): string {
  const trimmed = text.trim();
  if (maxLen < 1) return '';
  // Fast path: already fits.
  if ([...trimmed].length <= maxLen) return trimmed;

  const chars = Array.from(trimmed);
  // Find the last index <= maxLen that ends on a word boundary (i.e. the char
  // at `end` is whitespace OR the char after `end` is whitespace). If no such
  // boundary exists (the first word is longer than maxLen), hard-cut at maxLen.
  let end = maxLen;
  // Walk back to the most recent whitespace so we never split a word.
  while (end > 0 && /\S/.test(chars[end - 1] ?? '') && !/\s/.test(chars[end] ?? '')) {
    end -= 1;
  }
  if (end === 0) {
    // The first word alone exceeds maxLen; hard-truncate at maxLen.
    end = maxLen;
  }
  const kept = chars.slice(0, end).join('').trimEnd();
  return `${kept}...`;
}
