/**
 * Wiki type definitions.
 *
 * Mirrors the Rust structs in `src-tauri/src/commands/wiki_cmd.rs`.
 * Keep these in sync with the backend serialization (`#[serde(rename_all =
 * "camelCase")]`).
 */

/** Status payload returned by `wiki_get_status`. */
export interface WikiStatus {
  /** Always true after `wiki_init` has scaffolded the tree. */
  configured: boolean;
  /** Absolute path to the effective `wiki-root/` directory. */
  rootDir: string;
  /** Whether an explicit override is configured (vs derived default). */
  isCustom: boolean;
  /** Platform default path (derived from `fulltext_storage_dir`). */
  defaultPath: string;
  /** Count of `.md` files in `/raw` (top-level). */
  rawCount: number;
  /** Count of `.md` files in `/wiki` (recursive). */
  pageCount: number;
  /** Whether the included article corpus changed since the last ingest. */
  needsRefresh: boolean;
  /** Number of included articles (the raw input set for the wiki). */
  includedArticleCount: number;
  /** Whether the wiki root has been initialized (AGENTS.md present). */
  initialized: boolean;
}

/** Result returned by `wiki_init`. */
export interface WikiInitResult {
  rootDir: string;
  /** true if AGENTS.md did not exist before this call. */
  created: boolean;
}

/** Info returned by `wiki_get_root_dir` / `wiki_set_root_dir`. */
export interface WikiRootInfo {
  effectivePath: string;
  isCustom: boolean;
  defaultPath: string;
}

/** Page types in the wiki. Mirrors the frontmatter `type` field. */
export type WikiPageType = 'concept' | 'author' | 'method' | 'synthesis' | 'source';

/** Page status values. Mirrors the frontmatter `status` field. */
export type WikiPageStatus = 'draft' | 'reviewed' | 'stale';

/** Which content fallback was used for a raw/source page. */
export type WikiContentSource = 'full_text' | 'ai_summary' | 'abstract' | 'metadata';

/** Result of `wiki_export_raw`: counts of articles + user files processed. */
export interface RawExportReport {
  articlesWritten: number;
  articlesSkipped: number;
  userFilesWritten: number;
  userFilesSkipped: number;
  userFilesUnsupported: string[];
}

/** Issue severity from the lint engine. */
export type LintSeverity = 'error' | 'warning' | 'info';

/** Issue category from the lint engine. */
export type LintKind =
  | 'broken-link'
  | 'orphan-page'
  | 'duplicate-slug'
  | 'missing-frontmatter'
  | 'missing-field'
  | 'stale-page';

/** A single lint issue. */
export interface LintIssue {
  page: string;
  slug: string;
  severity: LintSeverity;
  kind: LintKind;
  message: string;
}

/** Full lint report returned by `wiki_lint`. */
export interface LintReport {
  pageCount: number;
  issueCount: number;
  errors: number;
  warnings: number;
  infos: number;
  issues: LintIssue[];
  slugs: string[];
}

/** Progress event payload from `wiki:progress`. */
export interface WikiProgress {
  step: number;
  totalSteps: number;
  message: string;
}

/** A raw source article's metadata for reference resolution. */
export interface WikiSourceInfo {
  id: string;
  title: string;
  authors: string[];
  year: number | null;
  doi: string | null;
}

/** Lightweight page summary for the sidebar list (no body). */
export interface WikiPageSummary {
  slug: string;
  title: string;
  pageType: string;
  status: string;
  summary: string;
}

/** Result of `wiki_ingest`: pages written + errors. */
export interface IngestReport {
  rawSourcesRead: number;
  pagesWritten: number;
  pagesSkipped: number;
  sourceCharsTruncated: boolean;
  errors: string[];
}

/** A graph node (wiki page) for the graph view. */
export interface GraphNode {
  slug: string;
  title: string;
  pageType: string;
  inbound: number;
  outbound: number;
}

/** A directed graph edge ([[wikilink]] from source to target). */
export interface GraphEdge {
  source: string;
  target: string;
}

/** The full wiki link graph (nodes + edges). */
export interface WikiGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  orphanCount: number;
}

/** A chat message for `wiki_chat` (role + content). */
export interface WikiChatMessage {
  role: 'user' | 'assistant';
  content: string;
}

/** A wiki page returned by `wiki_get_page`. */
export interface WikiPage {
  slug: string;
  title: string;
  pageType: string;
  status: string;
  summary: string;
  body: string;
  filePath: string;
  sourceArticles: string | null;
}

/** A search hit from `wiki_search` (FTS5 BM25). */
export interface WikiPageHit {
  slug: string;
  title: string;
  summary: string;
  pageType: string;
  sourceArticles: string;
  filePath: string;
  rank: number;
}

/** A raw source entry returned by `wiki_list_raw_files`. */
export interface RawFileEntry {
  path: string;
  title: string;
  slug: string;
  sourceKind: string;
  sourceFile: string | null;
  status: string;
}
