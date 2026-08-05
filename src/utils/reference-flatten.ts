import type { ArticleReference } from '@/types';

/** Raw shape returned by the Rust `get_article_references` IPC command. */
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
    referenceType?: string | null;
  } | null;
}

/**
 * Map one raw IPC reference `{ linkId, referenceType, paper: {...} }` into a flat
 * `ArticleReference`. Total: every `??` default guarantees a well-formed result
 * even when `paper` is null. Grouped: link-level, match info, biblio core
 * (title→publisher, authors/keywords default `[]`), metrics
 * (citationCount→numCited, referenceCount→numReferences), provenance
 * (boolean hasFullText, createdAt→importedAt, paper.referenceType→publicationType).
 *
 * Cyclomatic score (25) is a false positive: every `??` is counted as a branch
 * by the analyzer but there is no real control flow. Exhaustively covered by
 * `src/__tests__/references.test.ts` (43 tests). Inline suppression is the
 * documented remedy.
 */
// fallow-ignore-next-line complexity
function flattenOneReference(r: RawReference): ArticleReference {
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
    publicationType: p.referenceType ?? null,
  } satisfies ArticleReference;
}

/**
 * Flatten IPC response into `ArticleReference[]`. Thin `.map()` over
 * `flattenOneReference`, extracted for independent unit-testing.
 */
export function flattenRawReferences(raw: unknown[]): ArticleReference[] {
  const rawRefs = raw as RawReference[];
  return rawRefs.map(flattenOneReference);
}
