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

/** Strip UUIDs (8-4-4-4-12 hex) from a details string. Cleans dangling prepositions. */
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

/** Determine file type icon from filename extension. */
export function getFullTextFileIcon(fileName: string | null | undefined): string | null {
  if (!fileName) return null;
  const lower = fileName.toLowerCase();
  if (lower.endsWith('.pdf')) return 'picture_as_pdf';
  if (lower.endsWith('.txt')) return 'description';
  return 'draft';
}

/** Format list of author names, truncating to first author + "et al." past limit. */
export function formatAuthors(
  authors: string[] | null | undefined,
  limit = 3,
  separator = ', '
): string {
  if (!authors || !authors.length) return '';
  if (authors.length <= limit) return authors.join(separator);
  return `${authors[0]} et al.`;
}

/** Create DOI hyperlink if possible. */
export function doiLink(doi: string | null | undefined): string | undefined {
  if (!doi) return undefined;
  return doi.startsWith('http') ? doi : `https://doi.org/${doi}`;
}

/** Average occurrences/year across the active span (lastYear - firstYear + 1).
 *  Returns null when no year data. */
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

/** Convert short publication type code (JOUR, BOOK, etc.) to human label. */
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

/** Derive display label for storage-root: last path segment + trailing slash.
 *  Falls back to `Bango/` on empty/root-only paths. */
export function folderLabelFromPath(path: string): string {
  // Normalize backslashes (Windows) to forward slashes for uniform splitting,
  // then drop trailing separators so the last split is the real folder name.
  const normalized = path.replace(/\\/g, '/').replace(/\/+$/, '');
  const segments = normalized.split('/').filter((s) => s.length > 0);
  const last = segments[segments.length - 1];
  return last ? `${last}/` : 'Bango/';
}

/**
 * Truncate `text` to at most `maxLen` chars at last word boundary,
 * appending `...` when truncated. Hard-truncates if first word exceeds max.
 * Counts code points via `Array.from`, not UTF-16 units.
 *
 * @param maxLen - max characters excluding ellipsis. Must be >= 1.
 * @returns truncated string with `...` when shortened, or trimmed original.
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
