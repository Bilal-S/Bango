import { vi } from 'vitest';
import type { Article } from '@/types';

/**
 * Build a minimal Article for tests, with sensible defaults and overrides.
 * All optional fields default to null/empty so tests only specify what matters.
 */
export function makeArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: 'a1',
    sequenceId: 1,
    status: 'included',
    screeningError: false,
    title: 'Test Article',
    abstractText: 'An abstract.',
    authors: ['Smith, J.'],
    publicationYear: 2021,
    doi: null,
    journal: null,
    volume: null,
    issue: null,
    startPage: null,
    endPage: null,
    keywords: [],
    url: null,
    language: null,
    publisher: null,
    publisherCity: null,
    publisherAddress: null,
    issn: null,
    eissn: null,
    journalIndexId: null,
    referenceType: null,
    date: null,
    authorAddress: null,
    affiliation: null,
    accessionNumber: null,
    customField3: null,
    journalAbbreviation: null,
    journalIsoAbbreviation: null,
    notes: null,
    webOfScienceDb: null,
    userNotes: null,
    risExtras: null,
    duplicateOf: null,
    aiDecision: null,
    aiReasoning: null,
    aiConfidence: null,
    matchedInclusionCriteria: [],
    matchedExclusionCriteria: [],
    tags: [],
    labels: [],
    manualOverride: false,
    importSource: null,
    importedAt: '',
    changedAt: '',
    screenedAt: null,
    dataLength: null,
    tokenEstimate: null,
    actualTokens: null,
    fullText: null,
    fullTextAiSummary: null,
    numCited: null,
    numReferences: null,
    hasCitationDetails: false,
    hasReferenceDetails: false,
    hasFullText: false,
    fullTextFileName: null,
    hasFiguresOrTables: false,
    isTranslated: false,
    translationStatus: 'none',
    translationError: null,
    translatedAt: null,
    ...overrides,
  } as Article;
}

/** Minimal per-status counts shape `makeArticlesStore` seeds. */
export interface ArticlesStatusCounts {
  duplicate: number;
  working: number;
  included: number;
  rejected: number;
}

/**
 * Mock articles Pinia store for tests that mock `@/stores/articles`.
 * Override any member via `overrides` (partial deep merge on `byStatus`).
 */
export function makeArticlesStore(
  overrides: {
    byStatus?: Partial<ArticlesStatusCounts>;
    totalImported?: number;
    articles?: Article[];
    loading?: boolean;
    error?: string | null;
    fetchIfNeeded?: () => Promise<void>;
    invalidate?: () => void;
  } = {}
) {
  const byStatus = {
    duplicate: overrides.byStatus?.duplicate ?? 0,
    working: overrides.byStatus?.working ?? 0,
    included: overrides.byStatus?.included ?? 0,
    rejected: overrides.byStatus?.rejected ?? 0,
  };
  return {
    byStatus,
    totalImported: overrides.totalImported ?? 0,
    articles: overrides.articles ?? [],
    loading: overrides.loading ?? false,
    error: overrides.error ?? null,
    fetchIfNeeded: overrides.fetchIfNeeded ?? vi.fn(async () => {}),
    invalidate: overrides.invalidate ?? vi.fn(),
  };
}

/** Mock tags Pinia store for tests that mock `@/stores/tags`. */
export function makeTagsStore(
  overrides: {
    tags?: {
      id: string;
      name: string;
      source?: string;
      color?: string | null;
      articleCount?: number;
    }[];
    loading?: boolean;
    fetchTags?: () => Promise<void>;
    fetchIfNeeded?: () => Promise<void>;
  } = {}
) {
  return {
    tags: overrides.tags ?? [],
    loading: overrides.loading ?? false,
    fetchTags: overrides.fetchTags ?? vi.fn(async () => {}),
    fetchIfNeeded: overrides.fetchIfNeeded ?? vi.fn(async () => {}),
  };
}

/** Mock labels Pinia store for tests that mock `@/stores/labels`. */
export function makeLabelsStore(
  overrides: {
    labels?: {
      id: string;
      name: string;
      source?: string;
      color?: string | null;
      articleCount?: number;
    }[];
    loading?: boolean;
    fetchLabels?: () => Promise<void>;
    fetchIfNeeded?: () => Promise<void>;
  } = {}
) {
  return {
    labels: overrides.labels ?? [],
    loading: overrides.loading ?? false,
    fetchLabels: overrides.fetchLabels ?? vi.fn(async () => {}),
    fetchIfNeeded: overrides.fetchIfNeeded ?? vi.fn(async () => {}),
  };
}

/** happy-dom-safe localStorage shim. */
export function shimLocalStorage(): Storage {
  const store = new Map<string, string>();
  return {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => {
      store.set(k, v);
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    clear: () => store.clear(),
    key: (i: number) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  } as Storage;
}
