export interface Article {
  id: string;
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
  screenedAt: string | null;
}

export type ArticleStatus = 'duplicate' | 'working' | 'included' | 'rejected';
export type AiDecision = 'include' | 'exclude';

export interface ArticleCounts {
  all: number;
  duplicate: number;
  working: number;
  included: number;
  rejected: number;
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
  | 'ai_summary';

export type AuditSource = 'ai' | 'user' | 'system';

export interface LlmConfig {
  provider: LlmProvider;
  endpointUrl: string;
  apiKeyEncrypted: string | null;
  modelName: string;
  temperature: number;
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
  currentArticleTitle: string | null;
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
