import type { ArticleReference } from '@/types';

/**
 * Raw shape returned by the Rust backend's `get_article_references` IPC command.
 * Each item has a nested `paper` object with the actual metadata fields.
 */
export interface RawReference {
  linkId: string;
  parentArticleId: string;
  referenceType: string;
  paper: {
    id?: string;
    title?: string | null;
    abstractText?: string | null;
    authors?: string[] | null;
    publicationYear?: number | null;
    doi?: string | null;
    journal?: string | null;
    volume?: string | null;
    issue?: string | null;
    startPage?: string | null;
    endPage?: string | null;
    keywords?: string[] | null;
    url?: string | null;
    language?: string | null;
    publisher?: string | null;
    matchStatus?: string | null;
    matchedArticleId?: string | null;
    citationCount?: number | null;
    referenceCount?: number | null;
    hasFullText?: boolean | null;
    fullTextFileName?: string | null;
    importSource?: string | null;
    createdAt?: string | null;
  } | null;
}

/**
 * Flatten the nested IPC response `{ linkId, referenceType, paper: {...} }`
 * into the flat `ArticleReference` shape expected by Vue templates.
 *
 * This function is extracted so it can be unit-tested independently.
 */
export function flattenRawReferences(raw: unknown[]): ArticleReference[] {
  const rawRefs = raw as RawReference[];
  return rawRefs.map((r) => {
    const p = r.paper ?? ({} as NonNullable<RawReference['paper']>);
    return {
      id: p.id ?? r.linkId,
      referenceType: r.referenceType as ArticleReference['referenceType'],
      parentId: r.parentArticleId,
      matchStatus: (p.matchStatus ?? 'unmatched') as ArticleReference['matchStatus'],
      matchedArticleId: p.matchedArticleId ?? null,
      title: p.title ?? null,
      abstractText: p.abstractText ?? null,
      authors: p.authors ?? [],
      publicationYear: p.publicationYear ?? null,
      doi: p.doi ?? null,
      journal: p.journal ?? null,
      volume: p.volume ?? null,
      issue: p.issue ?? null,
      startPage: p.startPage ?? null,
      endPage: p.endPage ?? null,
      keywords: p.keywords ?? [],
      url: p.url ?? null,
      language: p.language ?? null,
      publisher: p.publisher ?? null,
      numCited: p.citationCount ?? null,
      numReferences: p.referenceCount ?? null,
      hasFullText: !!p.hasFullText,
      fullTextFileName: p.fullTextFileName ?? null,
      importSource: p.importSource ?? null,
      importedAt: p.createdAt ?? '',
    } satisfies ArticleReference;
  });
}
