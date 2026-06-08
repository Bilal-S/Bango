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
}

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
  | 'error';

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
  isRunning: boolean;
  currentArticleTitles: string[];
  elapsedMs: number;
  estimatedRemainingMs: number | null;
}

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
