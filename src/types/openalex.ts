/**
 * TypeScript interfaces for the OpenAlex API types + command payloads.
 * Mirrors the Rust types in `src-tauri/src/openalex/mod.rs`.
 */

export interface OpenAlexWork {
  id: string;
  doi: string | null;
  title: string | null;
  publicationYear: number | null;
  publicationDate: string | null;
  authorships: OpenAlexAuthorship[];
  primaryLocation: OpenAlexPrimaryLocation | null;
  abstractInvertedIndex: Record<string, number[]> | null;
  biblio: OpenAlexBiblio | null;
  citedByCount: number;
  language: string | null;
  keywords: OpenAlexKeyword[];
  type: string | null;
  openAccess: OpenAlexOpenAccess | null;
  isRetracted: boolean | null;
  referencedWorks: string[];
}

export interface OpenAlexAuthorship {
  authorPosition: string | null;
  author: OpenAlexAuthor;
  institutions: OpenAlexInstitution[];
}

export interface OpenAlexAuthor {
  displayName: string | null;
  id: string | null;
}

export interface OpenAlexInstitution {
  displayName: string | null;
  country: string | null;
}

export interface OpenAlexPrimaryLocation {
  source: OpenAlexSource | null;
  landingPageUrl: string | null;
  pdfUrl: string | null;
}

export interface OpenAlexSource {
  displayName: string | null;
  issnL: string | null;
  issn: string[] | null;
}

export interface OpenAlexBiblio {
  volume: string | null;
  issue: string | null;
  firstPage: string | null;
  lastPage: string | null;
}

export interface OpenAlexKeyword {
  displayName: string;
  score: number | null;
}

export interface OpenAlexOpenAccess {
  isOa: boolean | null;
  oaStatus: string | null;
  oaUrl: string | null;
}

export interface OpenAlexResultItem {
  work: OpenAlexWork;
  abstractText: string;
  snippet: string;
  alreadyInLibrary: boolean;
}

export interface OpenAlexSearchResponse {
  results: OpenAlexResultItem[];
  totalCount: number;
  page: number;
  perPage: number;
}

export interface OpenAlexFilters {
  yearFrom: number | null;
  yearTo: number | null;
  workTypes: string[];
  language: string | null;
  isOa: boolean;
  showRetracted: boolean;
}

export const DEFAULT_OPENALEX_FILTERS: OpenAlexFilters = {
  yearFrom: null,
  yearTo: null,
  workTypes: [],
  language: null,
  isOa: false,
  showRetracted: false,
};

export interface OpenAlexSettings {
  hasApiKey: boolean;
  mailto: string;
  retrieveReferences: boolean;
}

export interface OpenAlexSettingsInput {
  apiKey?: string;
  mailto?: string;
  retrieveReferences?: boolean;
}

export interface SmartSearchFilters {
  publicationYear: string | null;
  type: string[];
}

export interface SmartSearchQuery {
  searchQuery: string;
  suggestedFilters: SmartSearchFilters;
}

export const SORT_OPTIONS = [
  { label: 'Relevance', value: 'relevance_score:desc' },
  { label: 'Newest first', value: 'publication_date:desc' },
  { label: 'Oldest first', value: 'publication_date:asc' },
  { label: 'Most cited', value: 'cited_by_count:desc' },
  { label: 'Least cited', value: 'cited_by_count:asc' },
] as const;

export const PER_PAGE_OPTIONS = [10, 25, 50, 100] as const;
