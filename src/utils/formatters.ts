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
