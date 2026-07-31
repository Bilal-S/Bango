/**
 * Citation Finder types - frontend mirror of `src-tauri/src/citation_finder/mod.rs`.
 *
 * Contract for the 3 Tauri commands (`find_citations`, `cancel_citation_search`,
 * `get_citation_finder_readiness`) + the 3 events (`citation:progress`,
 * `citation:done`, `citation:error`). `CitationStyle` is re-exported from
 * `use-summary.ts` so the Citation Finder + AI Summary share one 5-style list.
 */

export type CitationStyle = 'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA';

/** How the pasted text is processed. Matches the Rust `CitationFinderMode`
 *  (snake_case serde rename): the literal wire tokens are `whole_block` /
 *  `per_statement`. The conceptual modes are documented as "whole-block" /
 *  "per-statement", but the IPC payload MUST use snake_case to match the
 *  backend's `#[serde(rename_all = "snake_case")]`. */
export type CitationFinderMode = 'whole_block' | 'per_statement';

/**
 * One matched citation: one article's best passage + the LLM classification.
 * Mirrors `CitationMatch` in `citation_finder/mod.rs`.
 */
export interface CitationMatch {
  articleId: string;
  title: string;
  authors: string[];
  publicationYear: number | null;
  journal: string | null;
  doi: string | null;
  /** The best-matching chunk text from the article. */
  matchedPassage: string;
  /** Section the matched passage came from. `null` for Text-derived chunks →
   *  the UI omits the `§…` badge. Abstract-only articles synthesize
   *  `"Abstract"`. */
  sectionOrigin: string | null;
  classification: 'validating' | 'opposing';
  /** 1-2 sentence LLM explanation of HOW the passage relates to the claim. */
  relevanceExplanation: string;
  /** `true` = the matched passage is taken out of context / selectively quoted
   *  in a way that MISREPRESENTS the source. v1 does not render this in the
   *  card; reserved for a future warning chip. */
  misrepresentsSource: boolean;
  /** 1-3 EXACT verbatim sentences from `matchedPassage` that justify the
   *  classification. Populated backend-side by a grounding gate
   *  (`ground_quotes`) that filters the LLM's `justifying_sentences` against
   *  the actual passage so paraphrases/hallucinations are dropped. Empty when
   *  the LLM omitted the field or none grounded → the card falls back to the
   *  full passage. */
  highlightedSentences: string[];
  /** User-facing "match %" - the COSINE (semantic) score from the recall
   *  layer, normalized from `[-1, 1]` to `[0, 1]`. Jaccard is internal-only. */
  confidence: number;
}

/**
 * One search result group. In whole-block mode there is a single group with
 * `claim: null`; in per-statement mode there is one group per claim.
 */
export interface CitationResult {
  claim: string | null;
  matches: CitationMatch[];
}

/** Readiness payload returned by `get_citation_finder_readiness`. Drives the
 *  toggle visibility (hidden when `providerSupportsEmbeddings === false`). */
export interface CitationFinderReadiness {
  totalArticles: number;
  embeddedCount: number;
  coveragePct: number;
  providerSupportsEmbeddings: boolean;
  statuses: string[];
}

/** Progress payload emitted via the `citation:progress` event and returned by
 *  the `find_citations` command. */
export interface CitationFinderProgress {
  /** `"preparing_embeddings"` (Phase B) | `"searching"` (Phase C).
   *  Underscored to match the Rust `CitationFinderProgress.phase` string
   *  values emitted by `search.rs` + `commands/citation_finder.rs`. */
  phase: 'preparing_embeddings' | 'searching';
  /** Phase-C sub-stage. Omitted during Phase B. Underscored to match Rust. */
  stage?: 'embedding_query' | 'ranking' | 'classifying';
  done: number;
  total: number;
  /** 0-100 across BOTH phases. */
  overallPercent: number;
  message: string;
  isRunning: boolean;
  isCancelled: boolean;
}

/** Default status filter: Working + Included checked, Rejected / Duplicate
 *  unchecked. Mirrors spec §8.7 ("Duplicates always excluded"). */
export const DEFAULT_CITATION_STATUSES: string[] = ['working', 'included'];
