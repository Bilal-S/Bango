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
    referenceType?: string | null;
  } | null;
}

/**
 * Map one raw IPC reference (nested `{ linkId, referenceType, paper: {...} }`)
 * into the flat `ArticleReference` shape expected by Vue templates.
 *
 * Every field carries an explicit default via `??` so the function is total:
 * it always produces a well-formed `ArticleReference` even when `paper` is
 * `null` or individual fields are missing. Field groups:
 *
 * - **Link-level** (`id`, `referenceType`, `parentId`) come from the wrapper.
 *   `id` falls back to `linkId` when `paper.id` is absent.
 * - **Match info** (`matchStatus`, `matchedArticleId`) defaults to
 *   `'unmatched'` / `null`.
 * - **Bibliographic core** (title through publisher) defaults to `null`
 *   except `authors` / `keywords`, which default to `[]`.
 * - **Metrics** are renamed: `citationCount` -> `numCited`,
 *   `referenceCount` -> `numReferences` (both default `null`).
 * - **Provenance**: `hasFullText` is coerced to boolean, `createdAt` is
 *   renamed to `importedAt` (default `''`), and `paper.referenceType`
 *   (distinct from the link-level `referenceType`) surfaces as
 *   `publicationType`.
 *
 * Extracted from `flattenRawReferences` so the field-defaulting logic is
 * independently unit-testable and the `.map()` stays trivial.
 *
 * @param r - the raw reference wrapper
 * @returns the flattened `ArticleReference`
 */
// Justification: this is a pure 1:1 field-mapping function with a SINGLE
// execution path. The cyclomatic score (25) is inflated because each
// nullish-coalescing `??` operator is counted as a separate branch by the
// analyzer, but there is no real control flow here - every `??` is an
// independent field default that always evaluates left-to-right. The
// function is exhaustively covered by the field-level characterization
// tests in `src/__tests__/references.test.ts` (43 tests, including the
// null-paper, missing-fields, and full-contract cases). Inline suppression
// is the documented remedy for this metric false positive - see the
// Fallow "Gotchas" reference.
//
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
 * Flatten the nested IPC response `{ linkId, referenceType, paper: {...} }`
 * into the flat `ArticleReference` shape expected by Vue templates.
 *
 * This function is a thin `.map()` over {@link flattenOneReference}, which
 * owns the per-field defaulting contract. Extracted so it can be unit-tested
 * independently.
 */
export function flattenRawReferences(raw: unknown[]): ArticleReference[] {
  const rawRefs = raw as RawReference[];
  return rawRefs.map(flattenOneReference);
}
