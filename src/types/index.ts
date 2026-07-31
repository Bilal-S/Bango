export interface Article {
  id: string;
  sequenceId: number;
  status: ArticleStatus;
  screeningError: boolean;
  title: string;
  abstractText: string;
  authors: string[];
  publicationYear: number | null;
  doi: string | null;
  journal: string | null;
  /** `journal_index.id` FK (resolved at import/edit time via ISSN/eISSN/title).
   *  `null` when the journal name is not in the local index (the UI shows an
   *  "(unrecognized)" annotation next to the Journal label in this case). */
  journalIndexId: string | null;
  volume: string | null;
  issue: string | null;
  startPage: string | null;
  endPage: string | null;
  keywords: string[];
  url: string | null;
  language: string | null;
  publisher: string | null;
  publisherCity: string | null;
  publisherAddress: string | null;
  issn: string | null;
  referenceType: string | null;
  date: string | null;
  authorAddress: string | null;
  affiliation: string | null;
  accessionNumber: string | null;
  customField3: string | null;
  journalAbbreviation: string | null;
  journalIsoAbbreviation: string | null;
  notes: string | null;
  webOfScienceDb: string | null;
  userNotes: string | null;
  risExtras: Record<string, unknown> | null;
  duplicateOf: string | null;
  aiDecision: AiDecision | null;
  aiReasoning: string | null;
  aiConfidence: number | null;
  matchedInclusionCriteria: string[];
  matchedExclusionCriteria: string[];
  tags: string[];
  labels: string[];
  manualOverride: boolean;
  importSource: string | null;
  importedAt: string;
  changedAt: string;
  screenedAt: string | null;
  /** Extracted full text of the article (populated on demand) */
  fullText: string | null;
  /** AI-generated summary with pertinent points and data */
  fullTextAiSummary: string | null;
  /** Total times cited (from N1 field during import) */
  numCited: number | null;
  /** Number of cited references (from N1 field during import) */
  numReferences: number | null;
  /** Whether this article has imported citation details */
  hasCitationDetails: boolean;
  /** Whether this article has imported reference details */
  hasReferenceDetails: boolean;
  /** Whether full text file has been attached */
  hasFullText: boolean;
  /** Name of the attached full text file */
  fullTextFileName: string | null;
  /** Whether the attached full text contains figure/table captions.
   * Computed once at attach time (backend `extract_captions`) and persisted on
   * the row so the "Describe Figures & Tables" button gate is cheap. */
  hasFiguresOrTables: boolean;
  /** True when the working text (title/abstract/full_text/chunks) has been
   * permanently rewritten to English (Plan-A translation). Originals are
   * preserved in `article_original_content` / `article_original_chunks`. */
  isTranslated: boolean;
  /** DB-backed queue progress: 'none' | 'queued' | 'running' | 'succeeded' | 'failed'. */
  translationStatus: TranslationStatus;
  /** Error message captured when `translationStatus === 'failed'`. */
  translationError: string | null;
  /** RFC3339 timestamp of the last successful translation. */
  translatedAt: string | null;
}

/** DB-backed translation-queue progress states (mirrors the Rust enum). */
export type TranslationStatus = 'none' | 'queued' | 'running' | 'succeeded' | 'failed';

/** Reference type for imported citations/references */
export type ReferenceType = 'citation' | 'reference';

/** Match status for a reference against the article library */
export type MatchStatus = 'unmatched' | 'matched' | 'not_in_library' | 'imported';

/** A citation or reference associated with an article */
export interface ArticleReference {
  id: string;
  referenceType: ReferenceType;
  parentId: string;
  matchStatus: MatchStatus;
  /** ID of the article this reference was matched/promoted to (null if unmatched) */
  matchedArticleId: string | null;
  title: string | null;
  abstractText: string | null;
  authors: string[];
  publicationYear: number | null;
  doi: string | null;
  journal: string | null;
  volume: string | null;
  issue: string | null;
  startPage: string | null;
  endPage: string | null;
  keywords: string[];
  url: string | null;
  language: string | null;
  publisher: string | null;
  numCited: number | null;
  numReferences: number | null;
  hasFullText: boolean;
  fullTextFileName: string | null;
  importSource: string | null;
  importedAt: string;
  publicationType: string | null;
}

export type ArticleStatus = 'duplicate' | 'working' | 'included' | 'rejected';
export type AiDecision = 'include' | 'exclude';

export interface ArticleCounts {
  all: number;
  duplicate: number;
  working: number;
  included: number;
  rejected: number;
  error: number;
  references: number;
}

export interface ResearchAim {
  id: string;
  text: string;
  createdAt: string;
}

export interface Criterion {
  id: string;
  criterionType: CriterionType;
  text: string;
  priority: Priority;
  createdAt: string;
}

export type CriterionType = 'inclusion' | 'exclusion';
export type Priority = 'critical' | 'high' | 'standard' | 'low' | 'optional';

export interface Tag {
  id: string;
  name: string;
  source: TagSource;
  color: string | null;
}

export type TagSource = 'ai_suggested' | 'user_created' | 'ris_keyword';

export interface TagWithCount extends Tag {
  articleCount: number;
}

export interface Label {
  id: string;
  name: string;
  source: LabelSource;
  color: string | null;
}

export type LabelSource = 'ai_generated' | 'user_created';

export interface LabelWithCount extends Label {
  articleCount: number;
}

/**
 * Result of a tag/label merge (`merge_tag` / `merge_label` commands). The
 * precise counts are computed inside the destructive merge; the pre-confirm
 * dialog shows an honest upper bound (`from.articleCount`), and these values
 * surface in the success toast.
 */
export interface MergeResult {
  fromName: string;
  intoName: string;
  /** Articles whose tag/label link genuinely moved. */
  reassignedCount: number;
  /** Articles that already had the survivor and were silently de-linked. */
  alreadyHadSurvivorCount: number;
}

export interface AuditEntry {
  id: string;
  articleId: string;
  timestamp: string;
  action: AuditAction;
  fromStatus: string | null;
  toStatus: string | null;
  details: string | null;
  source: AuditSource;
  articleTitle: string | null;
}

export type AuditAction =
  | 'import'
  | 'dedup_merge'
  | 'dedup_flag'
  | 'status_change'
  | 'note_add'
  | 'tag_add'
  | 'tag_remove'
  | 'label_add'
  | 'label_remove'
  | 'criteria_match'
  | 'ai_screen'
  | 'manual_override'
  | 'ai_summary'
  | 'reference_import'
  | 'reference_match'
  | 'error'
  | 'translation'
  | 'translation_error'
  | 'ai_screen_clear'
  | 'metadata_edit';

export type AuditSource = 'ai' | 'user' | 'system';

export interface LlmConfig {
  provider: LlmProvider;
  endpointUrl: string;
  apiKeyEncrypted: string | null;
  modelName: string;
  temperature: number;
  skipTemperature: boolean;
  maxConcurrentRequests: number;
  requestDelayMs: number;
  contextWindowTokens: number;
}

export type LlmProvider =
  | 'openai'
  | 'anthropic'
  | 'google'
  | 'mistralAi'
  | 'zAi'
  | 'llamaCpp'
  | 'ollama'
  | 'lmStudio'
  | 'custom';

export interface HealthCheck {
  status: string;
  articleCount: number;
}

export interface ScreeningProgress {
  total: number;
  completed: number;
  included: number;
  rejected: number;
  errors: number;
  /** Articles deferred due to transient LLM errors (429/5xx/timeout/transport).
   *  Not counted in `completed` or `errors` - left unscreened for the next run. */
  deferred?: number;
  /** Fatal error that stopped the run (e.g. auth failure, consecutive transient
   *  failures). When set, `isRunning` is false and the UI shows a red banner. */
  fatalError?: string | null;
  /** Non-fatal warning (e.g. "LLM responding slowly"). When set, the UI shows a
   *  yellow banner. The run continues. Cleared on next success. */
  warning?: string | null;
  isRunning: boolean;
  currentArticleTitles: string[];
  elapsedMs: number;
  estimatedRemainingMs: number | null;
  /** Tier 3 two-stage: stage label sub-line (e.g. "Stage 2: 3/12 borderline"). */
  stage?: string | null;
  /** Tier 3 two-stage: per-stage total (borderline article count). */
  stageTotal?: number | null;
  /** Coarse run-phase label for the progress-bar sub-line so the UI shows
   *  *which* phase is in flight, not just a frozen percentage. Values:
   *  `"preparing:translating"`, `"preparing:chunking"`, `"screening"`,
   *  `"stage2"`. `null` when no run is active. Diagnostics-only. */
  phase?: string | null;
}

/**
 * Tier 3 screening mode (`abstract` | `enhanced` | `two_stage`). Mirrors the
 * Rust `ScreeningMode` enum (`serde(rename_all = "snake_case")`).
 */
export type ScreeningMode = 'abstract' | 'enhanced' | 'two_stage';

export interface ScreeningReadiness {
  totalWorking: number;
  totalUnscreened: number;
  hasAims: boolean;
  hasInclusion: boolean;
  hasExclusion: boolean;
  hasLlmConfig: boolean;
  tokenWarning: string | null;
  progress: ScreeningProgress | null;
}

/** A reference paper from the global reference_papers table (for References tab) */
export interface ReferencePaperQuery {
  id: string;
  title: string | null;
  abstractText: string | null;
  authors: string[];
  publicationYear: number | null;
  doi: string | null;
  journal: string | null;
  volume: string | null;
  issue: string | null;
  startPage: string | null;
  endPage: string | null;
  keywords: string[];
  url: string | null;
  language: string | null;
  publisher: string | null;
  matchStatus: MatchStatus;
  matchedArticleId: string | null;
  citationCount: number;
  referenceCount: number;
  importSource: string | null;
  createdAt: string;
  referenceType: string | null;
}

/** Result from querying reference papers with pagination */
export interface ReferencePaperQueryResult {
  papers: ReferencePaperQuery[];
  total: number;
}

/** A linked article that references/cites a paper (for References tab detail view) */
export interface LinkedArticleInfo {
  id: string;
  title: string;
  authors: string[];
  publicationYear: number | null;
  journal: string | null;
  referenceType: ReferenceType;
}

/** Progress state for batch reference scraping across all included articles */
export interface BatchRefScrapingProgress {
  /** Total included articles to evaluate */
  total: number;
  /** Articles processed so far */
  completed: number;
  /** Articles that required scraping + importing */
  scraped: number;
  /** Articles skipped (already have refs/citations, no DOI, or no missing data) */
  skipped: number;
  /** Articles that failed with errors */
  errors: number;
  /** Whether batch is currently running */
  isRunning: boolean;
  /** Title of the currently processing article */
  currentArticleTitle: string;
}

/**
 * A single `journal_index` hit returned by the `search_journal_index` command.
 * Powers the article-metadata journal autocomplete. Distinct from any
 * bibliometric aggregate type.
 */
export interface JournalIndexMatch {
  id: string;
  journalTitle: string;
  issn: string | null;
  eissn: string | null;
  publisherName: string | null;
}

/**
 * A structured suggestion row consumed by `suggest-input.vue` in its
 * `options` mode. When provided, the component renders `label` (bold) +
 * optional `sublabel` (muted) + optional `badge` (mono pill), and emits the
 * full object as the second `select` argument so the parent can read the
 * `id`. Used by the journal autocomplete.
 */
export interface SuggestOption {
  id: string;
  label: string;
  sublabel?: string;
  badge?: string;
}
