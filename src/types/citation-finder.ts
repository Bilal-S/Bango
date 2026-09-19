/**
 * Citation Finder types - frontend mirror of `src-tauri/src/citation_finder/mod.rs`.
 *
 * Contract for the 3 Tauri commands (`find_citations`, `cancel_citation_search`,
 * `get_citation_finder_readiness`) + the 3 events (`citation:progress`,
 * `citation:done`, `citation:error`). `CitationStyle` is re-exported from
 * `use-summary.ts` so the Citation Finder + AI Summary share one 5-style list.
 */

export type CitationStyle = 'APA' | 'MLA' | 'Chicago' | 'IEEE' | 'AMA';

/** How pasted text is processed. Matches Rust enum (snake_case on wire). */
export type CitationFinderMode = 'whole_block' | 'per_statement';

/** Articles-to-Search status selection. Duplicates are always excluded. */
export interface CitationStatusFlags {
  working: boolean;
  included: boolean;
  rejected: boolean;
}

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
  /** Best-matching chunk text from article. */
  matchedPassage: string;
  /** Section origin. `null` for Text → UI omits `§` badge. Abstract-only → "Abstract". */
  sectionOrigin: string | null;
  classification: 'validating' | 'opposing';
  /** LLM explanation of how passage relates to claim. */
  relevanceExplanation: string;
  /** True = passage taken out of context, misrepresents source. v1 reserved. */
  misrepresentsSource: boolean;
  /** 1-3 EXACT verbatim sentences from passage that justify classification.
   *  Filtered by `ground_quotes`; paraphrases/hallucinations dropped. Empty →
   *  card falls back to full passage. */
  highlightedSentences: string[];
  /** User-facing match % (cosine score from recall, normalized to [0,1]). */
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

/** Readiness payload driving toggle visibility/disabled state. Toggle renders
 *  disabled (not hidden) when `embeddingStatus === 'disabled'` - except when
 *  `embeddingBackend === 'bango_local'`, where a missing install stays
 *  clickable so the contextual download prompt can fire. */
export interface CitationFinderReadiness {
  totalArticles: number;
  embeddedCount: number;
  coveragePct: number;
  providerSupportsEmbeddings: boolean;
  statuses: string[];
  /** Triple-state: `'unknown'` (not probed), `'enabled'`, `'disabled'`. */
  embeddingStatus: 'unknown' | 'enabled' | 'disabled';
  /** Last-working embedding model name. `null` when not probed or disabled. */
  embeddingModel: string | null;
  /** Machine-local backend selection: `'configured_provider' | 'bango_local'`.
   *  With `bango_local` the chat-provider static check is skipped and
   *  `localReady` drives the contextual prompt. */
  embeddingBackend: 'configured_provider' | 'bango_local';
  /** When `embeddingBackend === 'bango_local'`: whether the local
   *  components are installed + healthy (local backend only; always `false`
   *  for the cloud backend). */
  localReady: boolean;
  /** Whether the CHAT provider (when configured) statically supports an
   *  embedding API - drives the contextual prompt's "Use Configured
   *  Provider" option (hidden for Anthropic/Z.AI). */
  chatProviderSupportsEmbeddings: boolean;
}

/** Model-mismatch payload. `null` when rows match current model. Drives
 *  confirmation dialog before search (stale vectors = silent zero hits). */
export interface EmbeddingModelMismatch {
  currentModel: string;
  storedModel: string;
  /** Total stored rows (context for "re-embed N rows" dialog). */
  storedRowCount: number;
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
  /** Present ONLY on the final search event: how many candidates the funnel
   *  reviewed vs. matched vs. dropped as not related (transparency for
   *  "why didn't my article show up"). Mirrors Rust `CitationFunnel`. */
  funnel?: CitationFunnel;
}

/** Phase-C funnel counts (mirrors `CitationFunnel` in
 *  `citation_finder/mod.rs`). */
export interface CitationFunnel {
  /** Recall hits across all claims (pre-gate). */
  recalled: number;
  /** Per-claim passage-evidence survivors summed across claims. */
  passageSurvivors: number;
  /** Distinct articles sent to the LLM (containment top-15 union cosine
   *  top-5, capped at 20). */
  finalists: number;
  /** LLM outputs that produced a validating/opposing match. */
  classified: number;
  /** LLM outputs dropped because the classification was `unrelated` (or
   *  unparseable). */
  droppedUnrelated: number;
}
